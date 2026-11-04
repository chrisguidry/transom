//! Signing the app in. The app opens `/login?app={challenge}&callback={url}`
//! in a Custom Tab, with a PKCE S256 challenge it made, and the person
//! signs in there as in a browser. transom then sends the Custom Tab to
//! the app's callback with a one-time code in place of a session cookie,
//! and the app POSTs the code and its PKCE verifier to `/login/app` for
//! the session's token. The verifier proves the app that redeems the code
//! is the one that began the sign-in, as RFC 8252 asks of a native app.
//!
//! - `app-code:{digest}`, a hash of one code not yet redeemed: the person
//!   its session will be for, and the app's challenge. It is keyed by a
//!   digest of the code, so Redis holds nothing the app could present.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use super::SESSION_PREFIX;
use super::sessions::{self, Kind, SignedIn, hashed, random};
use crate::app::{App, Failure};

/// How long a code waits to be redeemed: long enough for the app to come
/// to the front and POST it.
const CODE_LIFETIME: i64 = 60;

/// A sign-in the app began: its PKCE challenge, and the allowed callback
/// it asked for.
pub(crate) struct AppSignIn {
    pub(crate) challenge: String,
    pub(crate) callback: url::Url,
}

impl AppSignIn {
    /// The app's sign-in, if the request began one. A challenge must be an
    /// S256 digest, and the callback one of the settings' `app_callbacks`,
    /// so a sign-in link cannot send a person's code to another site.
    pub(crate) fn requested(
        app: &App,
        challenge: Option<String>,
        callback: Option<String>,
    ) -> Result<Option<Self>, Failure> {
        if challenge.is_none() && callback.is_none() {
            return Ok(None);
        }
        let challenge = challenge.filter(|challenge| {
            challenge.len() == 43
                && challenge
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        });
        let callback = callback
            .and_then(|callback| url::Url::parse(&callback).ok())
            .filter(|callback| app.sign_in_callbacks.contains(callback));
        let (Some(challenge), Some(callback)) = (challenge, callback) else {
            return Err(Failure::new(
                StatusCode::BAD_REQUEST,
                "the app signs in with app, a PKCE S256 challenge, and callback, \
                 one of this server's app_callbacks",
            ));
        };
        Ok(Some(Self {
            challenge,
            callback,
        }))
    }

    /// Mints the one-time code for `person`'s session, and returns the
    /// callback that carries it to the app.
    pub(super) async fn hand_off(self, app: &App, person: &SignedIn) -> Result<String, Failure> {
        let code = random();
        let key = app.store.key(&format!("app-code:{}", hashed(&code)));
        let mut connection = app.store.connection().await?;
        let mut pipe = redis::pipe();
        pipe.atomic()
            .hset_multiple(
                &key,
                &[
                    ("issuer", person.issuer.as_str()),
                    ("subject", &person.subject),
                    ("name", &person.name),
                    ("refresh", &person.refresh),
                    ("challenge", &self.challenge),
                ],
            )
            .expire(&key, CODE_LIFETIME);
        pipe.query_async::<()>(&mut connection).await?;
        let mut location = self.callback;
        location.query_pairs_mut().append_pair("code", &code);
        Ok(location.into())
    }
}

#[derive(Deserialize)]
pub(crate) struct Redemption {
    code: String,
    verifier: String,
}

/// Trades a one-time code and the PKCE verifier behind its challenge for
/// a session's token. A code is spent by its first attempt, right or
/// wrong, so a verifier cannot be guessed at.
pub(crate) async fn redeem(
    State(app): State<Arc<App>>,
    Json(redemption): Json<Redemption>,
) -> Result<Response, Failure> {
    let key = app
        .store
        .key(&format!("app-code:{}", hashed(&redemption.code)));
    let mut connection = app.store.connection().await?;
    let (mut stored,): (std::collections::HashMap<String, String>,) = redis::pipe()
        .atomic()
        .hgetall(&key)
        .del(&key)
        .ignore()
        .query_async(&mut connection)
        .await?;
    let mut field = |name: &str| stored.remove(name);
    let challenge = field("challenge");
    let (Some(issuer), Some(subject), true) = (
        field("issuer"),
        field("subject"),
        challenge == Some(hashed(&redemption.verifier)),
    ) else {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "this code was spent, has expired, or is for another verifier; sign in again",
        ));
    };
    let person = SignedIn {
        issuer,
        subject,
        name: field("name").unwrap_or_default(),
        refresh: field("refresh").unwrap_or_default(),
    };
    let token = sessions::start(&app, &person, Kind::App).await?;
    Ok(Json(json!({ "token": format!("{SESSION_PREFIX}{token}") })).into_response())
}
