//! Refreshing a session with its issuer, which is how transom learns that
//! the person may no longer sign in.
//!
//! Every server sharing a Redis may see a session come due at once, and an
//! issuer that rotates refresh tokens refuses one used twice, so a refresh
//! is claimed, called, and written back:
//!
//! 1. A script claims the refresh by marking the session refreshed now, so
//!    no other request, on this server or another, tries for fifteen
//!    minutes, whatever this one's call to the issuer comes to.
//! 2. The call to the issuer and the write-back run in a task of their
//!    own, which the request waits for only so long. A request that stops
//!    waiting carries on with the session, and the task still writes back
//!    what the issuer answers.
//! 3. A second script writes the new refresh token back only if the
//!    session still holds the one that was sent, and the session ends on
//!    `invalid_grant` only under the same condition.
//!
//! The issuer forgets the old refresh token when it rotates it, so an
//! answer that never arrives leaves the session holding a refresh token
//! the issuer has replaced, which ends the session at its next refresh.
//! That happens when the server stops while the task waits for the issuer,
//! up to the issuer's own timeouts of about half a minute, or when the
//! issuer's answer is lost on the way. It costs the person one sign-in.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use redis::Script;

use super::issuer::Issuer;
use crate::store::Connection;

use super::issuer::Declined;
use super::sessions::{self, Ended, key, now};
use crate::app::{App, Failure};
use crate::metrics::{Refresh, Refusal};

/// How often a session checks with its issuer that the person may still
/// sign in, by using its refresh token.
pub(crate) const REFRESH_EVERY: Duration = Duration::from_mins(15);

/// The OAuth errors that say transom's own client is configured wrong,
/// which the person cannot fix by signing in again.
const MISCONFIGURED: [&str; 3] = [
    "invalid_client",
    "unauthorized_client",
    "unsupported_grant_type",
];

/// Claims a session's refresh: returns its refresh token and marks it
/// refreshed at `ARGV[1]`, if it has one and its last refresh was at
/// least `ARGV[2]` milliseconds before; otherwise returns nothing.
static CLAIM: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        local refresh = redis.call('HGET', KEYS[1], 'refresh')
        if not refresh or refresh == '' then
            return false
        end
        local refreshed = tonumber(redis.call('HGET', KEYS[1], 'refreshed')) or 0
        if tonumber(ARGV[1]) - refreshed < tonumber(ARGV[2]) then
            return false
        end
        redis.call('HSET', KEYS[1], 'refreshed', ARGV[1])
        return refresh
        ",
    )
});

/// Writes the refresh token `ARGV[2]` and the time `ARGV[3]` the issuer
/// confirmed the session, if the session still holds the refresh token
/// `ARGV[1]` that was sent. A session that ended holds none.
static WRITE_BACK: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        if redis.call('HGET', KEYS[1], 'refresh') ~= ARGV[1] then
            return 0
        end
        redis.call('HSET', KEYS[1], 'refresh', ARGV[2], 'confirmed', ARGV[3])
        return 1
        ",
    )
});

/// Trades the session's refresh token `sent` with `issuer`, and writes the
/// one it rotated to back to the session at `key`. It runs as a task of its
/// own, so that a request that stops waiting for a slow issuer still keeps
/// the token the issuer rotated to: the issuer has already forgotten
/// `sent`, and the next refresh with it would end the session.
async fn refresh(
    issuer: std::sync::Arc<Issuer>,
    mut connection: Connection,
    key: String,
    sent: String,
) -> Result<redis::RedisResult<i64>, Declined> {
    let tokens = issuer.refresh(&sent).await?;
    let refresh = tokens["refresh_token"].as_str().unwrap_or(&sent);
    Ok(WRITE_BACK
        .key(key)
        .arg(&sent)
        .arg(refresh)
        .arg(now())
        .invoke_async(&mut connection)
        .await)
}

/// Refreshes the session with `digest`, whose hash is `fields`, if it is
/// due, and returns why it ended if the refresh ended it.
pub(super) async fn when_due(
    app: &App,
    digest: &str,
    fields: &HashMap<String, String>,
) -> Result<Option<Ended>, Failure> {
    let every = i64::try_from(REFRESH_EVERY.as_millis()).expect("fifteen minutes fits");
    let refreshed: i64 = fields
        .get("refreshed")
        .and_then(|at| at.parse().ok())
        .unwrap_or_default();
    // Most requests come between refreshes, and this saves each of them
    // the script.
    let has_refresh = fields.get("refresh").is_some_and(|token| !token.is_empty());
    if !has_refresh || now() - refreshed < every {
        return Ok(None);
    }
    let mut connection = app.store.connection().await?;
    let claimed: Option<String> = CLAIM
        .key(key(app, digest))
        .arg(now())
        .arg(every)
        .invoke_async(&mut connection)
        .await?;
    let Some(sent) = claimed else {
        return Ok(None);
    };
    let issuer_url = fields.get("issuer").map_or("", String::as_str);
    let Some(issuer) = app.issuers.by_url(issuer_url) else {
        sessions::end_digest(app, digest).await?;
        return Ok(Some(Ended {
            refusal: Refusal::UntrustedIssuer,
            issuer: "none".to_owned(),
            reason: "your issuer is no longer trusted",
        }));
    };
    let name = issuer.settings.name.as_str();
    let refreshing = tokio::spawn(refresh(
        issuer.clone(),
        connection.clone(),
        key(app, digest),
        sent.clone(),
    ));
    let answered = match tokio::time::timeout(issuer.refresh_within(), refreshing).await {
        Ok(joined) => joined.unwrap_or_else(|error| Err(Declined::Unavailable(error.to_string()))),
        Err(_) => Err(Declined::Unavailable(
            "the issuer did not answer in time".to_owned(),
        )),
    };
    let outcome = match answered {
        Ok(written) => {
            let _: i64 = written?;
            Refresh::Refreshed
        }
        Err(Declined::Refused(error)) if error == "invalid_grant" => {
            if sessions::end_refused(app, digest, &sent).await? {
                app.metrics.refreshed(Refresh::Ended, name);
                return Ok(Some(Ended {
                    refusal: Refusal::IssuerEnded,
                    issuer: name.to_owned(),
                    reason: "your issuer ended your session",
                }));
            }
            // The session holds a newer refresh token than the one
            // refused, so it carries on with that one.
            Refresh::Refused
        }
        Err(Declined::Refused(error)) if MISCONFIGURED.contains(&error.as_str()) => {
            tracing::error!(issuer = name, %error, "an issuer refused transom's client a refresh");
            Refresh::Refused
        }
        Err(Declined::Refused(error)) => {
            tracing::warn!(issuer = name, %error, "an issuer refused a refresh, and the session carries on");
            Refresh::Refused
        }
        Err(Declined::Unavailable(reason)) => {
            tracing::warn!(issuer = name, %reason, "a session could not be refreshed, and carries on");
            Refresh::Unavailable
        }
    };
    app.metrics.refreshed(outcome, name);
    Ok(None)
}
