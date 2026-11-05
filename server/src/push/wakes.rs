//! Waking phones that wake by FCM: a queue in Redis that every server
//! drains, of wake-ups for the wake-up gateway. A wake-up carries nothing
//! but the phone's registration token, and the app syncs from the server
//! when it wakes, so one wake-up brings everything that changed since the
//! last.
//!
//! - `wakes`, a sorted set of wake-up ids by when each is next due.
//! - `wake:{id}`, a hash of one wake-up's `identity` and `device`, how
//!   many `attempts` it has had, and when it was `first` queued. The token
//!   is read from the device at each attempt, so a phone that registers a
//!   new token is woken by it.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::time::Duration;

use aws_lc_rs::digest::{SHA256, digest};
use axum::http::header::RETRY_AFTER;
use redis::AsyncCommands;
use serde_json::{Value, json};

use super::devices::{self, Device, Wake};
use crate::app::{App, Failure};

/// How long transom keeps trying a wake-up before giving up on it.
const GIVE_UP_AFTER: i64 = 60 * 60 * 1000;

/// The longest wait between two attempts the gateway gave no time for.
const LONGEST_BACKOFF: i64 = 5 * 60 * 1000;

/// How long the gateway has to answer one attempt. It waits on FCM for up
/// to 10 seconds itself.
const TIMEOUT: Duration = Duration::from_secs(15);

/// Queues a wake-up for each of `identity`'s `devices` that wakes by FCM,
/// due at once.
pub(crate) async fn enqueue(
    app: &App,
    identity: &str,
    devices: &[(String, Device)],
    now: i64,
) -> Result<(), Failure> {
    let mut connection = app.store.connection().await?;
    let phones = devices
        .iter()
        .filter(|(_, device)| matches!(device.wake, Wake::Fcm(_)));
    for (device, _) in phones {
        let id = ulid::Ulid::generate().to_string();
        let mut pipe = redis::pipe();
        pipe.atomic()
            .hset_multiple(
                app.store.key(&format!("wake:{id}")),
                &[
                    ("identity", identity),
                    ("device", device),
                    ("attempts", "0"),
                    ("first", &now.to_string()),
                ],
            )
            .zadd(app.store.key("wakes"), &id, now);
        pipe.query_async::<()>(&mut connection).await?;
    }
    Ok(())
}

/// A short, stable name for `token`, from which the token cannot be
/// recovered: the same one the gateway logs, so the server's log lines
/// and the gateway's name the same phone. Anyone who holds a token may ask
/// the gateway to wake that phone, so no log line holds one.
fn fingerprint(token: &str) -> String {
    let mut name = String::new();
    for byte in &digest(&SHA256, token.as_bytes()).as_ref()[..6] {
        let _ = write!(name, "{byte:02x}");
    }
    name
}

/// What became of one attempt.
enum Attempt {
    /// The gateway took it, or it can never be delivered.
    Done,
    /// The gateway could not take it now: try again after this many
    /// milliseconds, when it said.
    Again(Option<i64>),
}

/// Makes one attempt at waking one phone.
async fn attempt(app: &App, fields: &Fields) -> Result<Attempt, Failure> {
    // A phone that is gone, or that no longer wakes by FCM, gets nothing
    // more.
    let phone = devices::one(app, &fields.identity, &fields.device).await?;
    let Some(Device {
        wake: Wake::Fcm(token),
        ..
    }) = phone
    else {
        return Ok(Attempt::Done);
    };
    let device = fingerprint(&token);
    let sent = app
        .http
        .post(&app.wake_gateway)
        .timeout(TIMEOUT)
        .json(&json!({ "token": token }))
        .send()
        .await;
    let response = match sent {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(%device, %error, "the wake-up gateway could not be reached");
            return Ok(Attempt::Again(None));
        }
    };
    let status = response.status();
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok()?.parse::<i64>().ok())
        .map(|seconds| seconds.saturating_mul(1000));
    let body: Value = response.json().await.unwrap_or_default();
    let reason = body["error"].as_str();
    Ok(match (status.as_u16(), reason) {
        (200..=299, _) => {
            tracing::info!(%device, %status, "the gateway woke a phone");
            Attempt::Done
        }
        // Only the gateway's own 404, which names its reason, means FCM
        // will never deliver to the token. A 404 from anything else, such
        // as a setting that names the wrong URL, must not forget phones.
        (404, Some(reason)) => {
            tracing::info!(%device, reason, "the gateway forgot a phone");
            devices::forget(app, &fields.identity, &fields.device).await?;
            Attempt::Done
        }
        // The gateway's limit for the token, FCM busy or failing, or a
        // proxy in front of the gateway that cannot reach it. A 502 is
        // left out: it is FCM refusing the wake-up for a reason that
        // trying again does not change.
        (429 | 500 | 501 | 503..=599, _) => {
            tracing::warn!(%device, %status, reason, "the wake-up gateway was busy");
            Attempt::Again(retry_after)
        }
        _ => {
            tracing::error!(%device, %status, reason, "the wake-up gateway refused a wake-up");
            Attempt::Done
        }
    })
}

/// One queued wake-up, as Redis holds it.
struct Fields {
    identity: String,
    device: String,
    attempts: i64,
    first: i64,
}

/// Attempts every wake-up due by `now`, rescheduling each the gateway could
/// not take after the time it gave, or with a backoff that doubles, up to
/// five minutes, and dropping one that has failed for an hour. A phone with
/// several wake-ups due is woken once, since one wake-up syncs everything.
/// Returns how many were attempted. Servers sharing a Redis can all run
/// this: removing a wake-up from the queue claims it.
pub(crate) async fn wake_due(app: &App, now: i64) -> Result<usize, Failure> {
    let mut connection = app.store.connection().await?;
    let due: Vec<String> = connection
        .zrangebyscore(app.store.key("wakes"), "-inf", now)
        .await?;
    let mut woken = HashSet::new();
    let mut attempted = 0;
    for id in due {
        let claimed: i64 = connection.zrem(app.store.key("wakes"), &id).await?;
        let key = app.store.key(&format!("wake:{id}"));
        let stored: HashMap<String, String> = connection.hgetall(&key).await?;
        let (1, Some(fields)) = (claimed, fields(&stored)) else {
            continue;
        };
        // The phone's first wake-up in this pass stands for the rest,
        // whether it was taken, dropped, or is to be tried again.
        if !woken.insert((fields.identity.clone(), fields.device.clone())) {
            let () = connection.del(&key).await?;
            continue;
        }
        attempted += 1;
        match attempt(app, &fields).await? {
            Attempt::Again(retry_after) if now - fields.first < GIVE_UP_AFTER => {
                let backoff = (1000_i64 << fields.attempts.min(22)).min(LONGEST_BACKOFF);
                let () = connection
                    .hset(&key, "attempts", fields.attempts + 1)
                    .await?;
                let () = connection
                    .zadd(
                        app.store.key("wakes"),
                        &id,
                        now + retry_after.unwrap_or(backoff),
                    )
                    .await?;
            }
            _ => {
                let () = connection.del(&key).await?;
            }
        }
    }
    Ok(attempted)
}

fn fields(stored: &HashMap<String, String>) -> Option<Fields> {
    let number = |name: &str| stored.get(name)?.parse().ok();
    Some(Fields {
        identity: stored.get("identity")?.clone(),
        device: stored.get("device")?.clone(),
        attempts: number("attempts")?,
        first: number("first")?,
    })
}
