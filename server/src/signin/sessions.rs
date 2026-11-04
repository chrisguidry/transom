//! Sessions: a person signed in through an issuer, held on the server so
//! a browser or the app carries only a random token.
//!
//! - `session:{digest}`, a hash of one session: its issuer, subject, name,
//!   refresh token, when it last refreshed, and its kind. It is keyed by a
//!   digest of the token, so Redis holds nothing a client could present.

use aws_lc_rs::digest::{SHA256, digest};
use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use redis::AsyncCommands;

use super::Verified;
use super::issuer::Rejection;
use crate::app::{App, Failure};

/// How long a browser's session lasts without being used.
pub(super) const BROWSER_LIFETIME: u64 = 30 * 24 * 60 * 60;

/// How long the app's session lasts without being used. A phone can go
/// months without asking transom anything while it still receives pushes,
/// and a person expects an installed app to stay signed in. The issuer
/// still decides: a session refreshes with it every fifteen minutes of
/// use, and ends when the issuer refuses. Forgetting the phone's device
/// ends it at once.
const APP_LIFETIME: u64 = 365 * 24 * 60 * 60;

/// How often a session checks with its issuer that the person may still
/// sign in, by using its refresh token.
const REFRESH_EVERY: i64 = 15 * 60 * 1000;

/// Who holds a session's token, which sets how long it lasts unused.
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Browser,
    App,
}

impl Kind {
    fn lifetime(self) -> i64 {
        match self {
            Self::Browser => BROWSER_LIFETIME,
            Self::App => APP_LIFETIME,
        }
        .cast_signed()
    }

    fn stored(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::App => "app",
        }
    }
}

/// The person an issuer signed in, as a new session records them.
pub(crate) struct SignedIn {
    /// The issuer's URL.
    pub(crate) issuer: String,
    pub(crate) subject: String,
    /// The value of the issuer's name claim, or empty.
    pub(crate) name: String,
    /// The issuer's refresh token, or empty when it gave none.
    pub(crate) refresh: String,
}

pub(super) fn random() -> String {
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("the system can make random bytes");
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The base64url SHA-256 of `value`: how Redis keys a secret, and the
/// PKCE S256 challenge of a verifier.
pub(crate) fn hashed(value: &str) -> String {
    URL_SAFE_NO_PAD.encode(digest(&SHA256, value.as_bytes()))
}

fn now() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

fn key(app: &App, digest: &str) -> String {
    app.store.key(&format!("session:{digest}"))
}

/// Starts a session for `person`, and returns its token.
pub(crate) async fn start(app: &App, person: &SignedIn, kind: Kind) -> Result<String, Failure> {
    let token = random();
    let key = key(app, &hashed(&token));
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
                ("refreshed", &now().to_string()),
                ("kind", kind.stored()),
            ],
        )
        .expire(&key, kind.lifetime());
    pipe.query_async::<()>(&mut connection).await?;
    Ok(token)
}

/// The caller of a request that carries the session `token`, refreshing
/// the session with its issuer when it is due. A session whose issuer
/// refuses the refresh ends; one whose issuer cannot be reached carries on.
pub(crate) async fn session(app: &App, token: &str) -> Result<Verified, Failure> {
    let mut connection = app.store.connection().await?;
    let digest = hashed(token);
    let key = key(app, &digest);
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
    let kind = match fields.get("kind").map(String::as_str) {
        Some("app") => Kind::App,
        _ => Kind::Browser,
    };
    let () = connection.expire(&key, kind.lifetime()).await?;
    Ok(Verified {
        issuer: issuer_url.clone(),
        subject: subject.clone(),
        name: fields.get("name").filter(|name| !name.is_empty()).cloned(),
        session: Some(digest),
    })
}

/// Ends the session whose token is `token`.
pub(crate) async fn end(app: &App, token: &str) -> Result<(), Failure> {
    end_digest(app, &hashed(token)).await
}

/// Ends the session whose token has the digest `digest`, as when the
/// device it registered is forgotten.
pub(crate) async fn end_digest(app: &App, digest: &str) -> Result<(), Failure> {
    let mut connection = app.store.connection().await?;
    let () = connection.del(key(app, digest)).await?;
    Ok(())
}
