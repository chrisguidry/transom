//! Signing a person in to a browser, as RFC 10017 (OAuth 2.0 for
//! Browser-Based Applications) describes its most secure pattern, the
//! Backend for Frontend: transom runs the authorization code flow with
//! PKCE as a confidential client, keeps the tokens on the server, and
//! gives the browser only a session cookie.
//!
//! - `login:{state}`, a hash of one sign-in in progress: the issuer, the
//!   PKCE verifier, the nonce, and the page to return to.
//! - `session:{digest}`, a hash of one session: its issuer, subject, name,
//!   refresh token, and when it last refreshed. It is keyed by a digest of
//!   the cookie's value, so Redis holds nothing a browser could present.

use std::sync::Arc;

use aws_lc_rs::digest::{SHA256, digest};
use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use axum::extract::{Path, Query, State};
use axum::http::header::{COOKIE, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use redis::AsyncCommands;
use serde::Deserialize;

use super::Verified;
use super::issuer::Rejection;
use crate::app::{App, Failure};

/// The session cookie, with the `__Host-Http-` prefix RFC 10017 asks for.
pub(crate) const SESSION_COOKIE: &str = "__Host-Http-transom";

/// The cookie that binds a sign-in in progress to the browser that began
/// it. It is `Lax`, unlike the session's, because the issuer sends the
/// browser back from another site.
const LOGIN_COOKIE: &str = "__Host-transom-login";

/// How long a session lasts without being used.
const SESSION_LIFETIME: u64 = 30 * 24 * 60 * 60;

/// How long a sign-in may take at the issuer.
const LOGIN_LIFETIME: u64 = 10 * 60;

/// How often a session checks with its issuer that the person may still
/// sign in, by using its refresh token.
const REFRESH_EVERY: i64 = 15 * 60 * 1000;

fn random() -> String {
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("the system can make random bytes");
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hashed(value: &str) -> String {
    URL_SAFE_NO_PAD.encode(digest(&SHA256, value.as_bytes()))
}

fn now() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

/// The value of the cookie `name` in a request's headers.
pub(crate) fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| {
            let (key, value) = pair.trim().split_once('=')?;
            (key == name).then(|| value.to_owned())
        })
}

fn set_cookie(name: &str, value: &str, same_site: &str, max_age: u64) -> String {
    format!("{name}={value}; Path=/; Secure; HttpOnly; SameSite={same_site}; Max-Age={max_age}")
}

/// Where the issuer sends a browser back to.
fn callback(app: &App, headers: &HeaderMap) -> String {
    let base = app.public_url.clone().unwrap_or_else(|| {
        let host = headers
            .get("host")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("localhost");
        format!("http://{host}")
    });
    format!("{base}/login/callback")
}

fn unavailable(rejection: Rejection) -> Failure {
    match rejection {
        Rejection::Invalid(reason) => Failure::new(StatusCode::UNAUTHORIZED, reason),
        Rejection::Unavailable(reason) => Failure::new(StatusCode::SERVICE_UNAVAILABLE, reason),
    }
}

/// Only a path on this server is a place to return to, so a sign-in link
/// cannot send a person elsewhere.
fn local(next: Option<&str>) -> String {
    next.filter(|next| next.starts_with('/') && !next.starts_with("//"))
        .unwrap_or("/")
        .to_owned()
}

#[derive(Deserialize)]
pub(crate) struct Next {
    next: Option<String>,
}

/// Sends the browser to the issuer named in the path to sign in.
pub(crate) async fn begin(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    Query(next): Query<Next>,
    headers: HeaderMap,
) -> Result<Response, Failure> {
    let issuer = app
        .issuers
        .named(&name)
        .filter(|issuer| issuer.settings.client_id.is_some())
        .ok_or_else(|| {
            Failure::new(
                StatusCode::NOT_FOUND,
                format!("no issuer {name} signs in browsers"),
            )
        })?;
    let endpoints = issuer.endpoints().await.map_err(unavailable)?;
    let (state, verifier, nonce) = (random(), random(), random());
    let challenge = URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()));
    let mut connection = app.store.connection().await?;
    let key = app.store.key(&format!("login:{state}"));
    let next = local(next.next.as_deref());
    let mut pipe = redis::pipe();
    pipe.atomic()
        .hset_multiple(
            &key,
            &[
                ("issuer", name.as_str()),
                ("verifier", &verifier),
                ("nonce", &nonce),
                ("next", &next),
            ],
        )
        .expire(&key, LOGIN_LIFETIME.cast_signed());
    pipe.query_async::<()>(&mut connection).await?;
    let mut url = url::Url::parse(&endpoints.authorization)
        .map_err(|error| Failure::new(StatusCode::SERVICE_UNAVAILABLE, error.to_string()))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair(
            "client_id",
            issuer.settings.client_id.as_deref().unwrap_or_default(),
        )
        .append_pair("redirect_uri", &callback(&app, &headers))
        .append_pair("scope", "openid profile email offline_access")
        .append_pair("state", &state)
        .append_pair("nonce", &nonce)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");
    Ok((
        StatusCode::SEE_OTHER,
        [
            (LOCATION, url.to_string()),
            (
                SET_COOKIE,
                set_cookie(LOGIN_COOKIE, &state, "Lax", LOGIN_LIFETIME),
            ),
        ],
    )
        .into_response())
}

#[derive(Deserialize)]
pub(crate) struct Returned {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

fn refused(reason: impl std::fmt::Display) -> Failure {
    Failure::new(StatusCode::UNAUTHORIZED, reason)
}

/// Finishes signing in when the issuer sends the browser back: trades the
/// code for tokens, checks the ID token, and starts a session.
pub(crate) async fn finish(
    State(app): State<Arc<App>>,
    Query(returned): Query<Returned>,
    headers: HeaderMap,
) -> Result<Response, Failure> {
    if let Some(error) = returned.error {
        return Err(refused(format!("the issuer did not sign you in: {error}")));
    }
    let (Some(code), Some(state)) = (returned.code, returned.state) else {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "the issuer sent no code",
        ));
    };
    if cookie(&headers, LOGIN_COOKIE).as_deref() != Some(state.as_str()) {
        return Err(refused("this sign-in began in another browser"));
    }
    let mut connection = app.store.connection().await?;
    let key = app.store.key(&format!("login:{state}"));
    let (issuer, verifier, nonce, next): (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = redis::cmd("HMGET")
        .arg(&key)
        .arg("issuer")
        .arg("verifier")
        .arg("nonce")
        .arg("next")
        .query_async(&mut connection)
        .await?;
    let () = connection.del(&key).await?;
    let (Some(issuer), Some(verifier), Some(nonce), Some(next)) = (issuer, verifier, nonce, next)
    else {
        return Err(refused("this sign-in has expired; begin again"));
    };
    let issuer = app
        .issuers
        .named(&issuer)
        .ok_or_else(|| refused("the issuer is no longer trusted"))?;
    let redirect_uri = callback(&app, &headers);
    let tokens = issuer
        .token_request(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect_uri),
            ("code_verifier", &verifier),
        ])
        .await
        .map_err(unavailable)?;
    let client_id = issuer.settings.client_id.as_deref().unwrap_or_default();
    let id_token = tokens["id_token"]
        .as_str()
        .ok_or_else(|| refused("the issuer sent no ID token"))?;
    let mut claims = issuer
        .verify_for(id_token, client_id)
        .await
        .map_err(unavailable)?;
    if claims.rest.get("nonce").and_then(|value| value.as_str()) != Some(nonce.as_str()) {
        return Err(refused("the ID token is not for this sign-in"));
    }
    let name = issuer
        .settings
        .name_claim
        .as_ref()
        .and_then(|claim| claims.rest.remove(claim))
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    let session = random();
    let key = app.store.key(&format!("session:{}", hashed(&session)));
    let refresh = tokens["refresh_token"].as_str().unwrap_or_default();
    let mut pipe = redis::pipe();
    pipe.atomic()
        .hset_multiple(
            &key,
            &[
                ("issuer", issuer.settings.url.as_str()),
                ("subject", &claims.sub),
                ("name", &name),
                ("refresh", refresh),
                ("refreshed", &now().to_string()),
            ],
        )
        .expire(&key, SESSION_LIFETIME.cast_signed());
    pipe.query_async::<()>(&mut connection).await?;
    let cookies = [
        set_cookie(SESSION_COOKIE, &session, "Strict", SESSION_LIFETIME),
        set_cookie(LOGIN_COOKIE, "", "Lax", 0),
    ];
    let mut response = (StatusCode::SEE_OTHER, [(LOCATION, next)]).into_response();
    for value in cookies {
        response.headers_mut().append(
            SET_COOKIE,
            value.parse().expect("a cookie is a header value"),
        );
    }
    Ok(response)
}

/// The caller of a request that carries a session cookie, refreshing the
/// session with its issuer when it is due. A session whose issuer refuses
/// the refresh ends; one whose issuer cannot be reached carries on.
pub(crate) async fn session(app: &App, value: &str) -> Result<Verified, Failure> {
    let mut connection = app.store.connection().await?;
    let key = app.store.key(&format!("session:{}", hashed(value)));
    let fields: std::collections::HashMap<String, String> = connection.hgetall(&key).await?;
    let (Some(issuer_url), Some(subject)) = (fields.get("issuer"), fields.get("subject")) else {
        return Err(super::unauthorized(
            Some("invalid_token"),
            &"your session has ended; sign in again",
        ));
    };
    let refreshed: i64 = fields
        .get("refreshed")
        .and_then(|at| at.parse().ok())
        .unwrap_or_default();
    let refresh = fields.get("refresh").filter(|refresh| !refresh.is_empty());
    if let (Some(refresh), true) = (refresh, now() - refreshed >= REFRESH_EVERY) {
        let Some(issuer) = app.issuers.by_url(issuer_url) else {
            let () = connection.del(&key).await?;
            return Err(super::unauthorized(
                Some("invalid_token"),
                &"your issuer is no longer trusted",
            ));
        };
        let form = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
        ];
        match issuer.token_request(&form).await {
            Ok(tokens) => {
                let refresh = tokens["refresh_token"].as_str().unwrap_or(refresh);
                let refreshed = now().to_string();
                let () = connection
                    .hset_multiple(&key, &[("refresh", refresh), ("refreshed", &refreshed)])
                    .await?;
            }
            Err(Rejection::Unavailable(reason)) => {
                tracing::warn!(%reason, "a session could not be refreshed, and carries on");
            }
            Err(Rejection::Invalid(_)) => {
                let () = connection.del(&key).await?;
                return Err(super::unauthorized(
                    Some("invalid_token"),
                    &"your issuer ended your session",
                ));
            }
        }
    }
    let () = connection
        .expire(&key, SESSION_LIFETIME.cast_signed())
        .await?;
    Ok(Verified {
        issuer: issuer_url.clone(),
        subject: subject.clone(),
        name: fields.get("name").filter(|name| !name.is_empty()).cloned(),
    })
}

/// Ends the browser's session.
pub(crate) async fn logout(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
) -> Result<Response, Failure> {
    if let Some(value) = cookie(&headers, SESSION_COOKIE) {
        let mut connection = app.store.connection().await?;
        let () = connection
            .del(app.store.key(&format!("session:{}", hashed(&value))))
            .await?;
    }
    let cleared = set_cookie(SESSION_COOKIE, "", "Strict", 0);
    // htmx follows a redirect inside its own request; HX-Redirect asks it
    // to take the whole page there instead.
    if headers.contains_key("hx-request") {
        return Ok((
            StatusCode::OK,
            [
                (
                    axum::http::HeaderName::from_static("hx-redirect"),
                    "/login".to_owned(),
                ),
                (SET_COOKIE, cleared),
            ],
        )
            .into_response());
    }
    Ok((
        StatusCode::SEE_OTHER,
        [(LOCATION, "/login".to_owned()), (SET_COOKIE, cleared)],
    )
        .into_response())
}
