//! Delivering push messages: a queue in Redis that every server drains. A
//! push service holds a message for a browser that is offline, so transom
//! retries only while the push service itself does not take it.
//!
//! - `pushes`, a sorted set of push ids by when each is next due.
//! - `push:{id}`, a hash of one push's `identity` and `device`, its
//!   `payload`, `urgency`, `ttl`, and `topic`, how many `attempts` it has
//!   had, and when it was `first` queued.

use std::time::Duration;

use redis::AsyncCommands;

use super::devices;
use super::encryption::{Receiver, encrypt};
use crate::app::{App, Failure};

/// How long transom keeps trying a push before giving up on it.
const GIVE_UP_AFTER: i64 = 60 * 60 * 1000;

/// The longest wait between two attempts.
const LONGEST_BACKOFF: i64 = 5 * 60 * 1000;

/// How long a push service has to answer one attempt.
const TIMEOUT: Duration = Duration::from_secs(10);

/// One message for one identity's devices, with the headers of RFC 8030.
pub(crate) struct Push<'a> {
    pub(crate) payload: &'a str,
    pub(crate) urgency: &'a str,
    /// How many seconds the push service keeps the message for an
    /// offline browser.
    pub(crate) ttl: i64,
    /// A later push with the same topic replaces one the push service
    /// still holds.
    pub(crate) topic: &'a str,
}

/// Queues `push` for each of `identity`'s `devices` that is pushed to, at
/// once.
pub(crate) async fn enqueue(
    app: &App,
    identity: &str,
    devices: &[(String, devices::Device)],
    push: &Push<'_>,
    now: i64,
) -> Result<(), Failure> {
    let mut connection = app.store.connection().await?;
    let pushed = devices
        .iter()
        .filter(|(_, device)| matches!(device.wake, devices::Wake::WebPush(_)));
    for (device, _) in pushed {
        // Pushes queued in the same millisecond tie in `pushes`, which then
        // orders them by id, so ids that increase deliver them in the
        // order they were queued: an arrival before the read that clears
        // it.
        let id = crate::exchanges::records::new_id();
        let mut pipe = redis::pipe();
        pipe.atomic()
            .hset_multiple(
                app.store.key(&format!("push:{id}")),
                &[
                    ("identity", identity),
                    ("device", device),
                    ("payload", push.payload),
                    ("urgency", push.urgency),
                    ("ttl", &push.ttl.to_string()),
                    ("topic", push.topic),
                    ("attempts", "0"),
                    ("first", &now.to_string()),
                ],
            )
            .zadd(app.store.key("pushes"), &id, now);
        pipe.query_async::<()>(&mut connection).await?;
    }
    Ok(())
}

/// What became of one attempt.
enum Attempt {
    /// The push service took it, or it can never be delivered.
    Done,
    /// The push service could not take it now.
    Again,
}

/// Makes one attempt at a push to one device.
async fn attempt(app: &App, fields: &Fields, now: i64) -> Result<Attempt, Failure> {
    // A device that is gone, that no longer wakes by push, or whose key no
    // longer takes a message, gets nothing more.
    let sealed = devices::one(app, &fields.identity, &fields.device)
        .await?
        .and_then(|device| {
            let devices::Wake::WebPush(subscription) = device.wake else {
                return None;
            };
            let (public, auth) = subscription.secrets()?;
            let body = encrypt(
                fields.payload.as_bytes(),
                &Receiver {
                    public: &public,
                    auth: &auth,
                },
            );
            Some((subscription.endpoint, body.ok()?))
        });
    let Some((endpoint, body)) = sealed else {
        return Ok(Attempt::Done);
    };
    // Only the push service's host is logged: the whole endpoint lets
    // anyone who holds it push to the browser.
    let host = url::Url::parse(&endpoint)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned));
    let service = host.as_deref();
    let subject = app.public_url.as_deref().unwrap_or("https://transom.sh");
    let vapid = app.push.vapid(app).await?;
    let request = app
        .http
        .post(&endpoint)
        .timeout(TIMEOUT)
        .header(
            "authorization",
            vapid.authorization(&endpoint, subject, now / 1000),
        )
        .header("content-encoding", "aes128gcm")
        .header("content-type", "application/octet-stream")
        .header("ttl", fields.ttl.to_string())
        .header("urgency", &fields.urgency)
        .header("topic", &fields.topic)
        .body(body);
    Ok(match request.send().await {
        Ok(response) if response.status().is_success() => {
            tracing::info!(service, status = %response.status(), "a push service took a message");
            Attempt::Done
        }
        // The browser unsubscribed, or its subscription expired.
        Ok(response) if matches!(response.status().as_u16(), 404 | 410) => {
            tracing::info!(service, status = %response.status(), "a push service forgot a device");
            devices::forget(app, &fields.identity, &fields.device).await?;
            Attempt::Done
        }
        Ok(response)
            if response.status().as_u16() == 429 || response.status().is_server_error() =>
        {
            tracing::warn!(service, status = %response.status(), "a push service was busy");
            Attempt::Again
        }
        Ok(response) => {
            tracing::warn!(service, status = %response.status(), "a push service refused a message");
            Attempt::Done
        }
        Err(error) => {
            tracing::warn!(service, %error, "a push message could not be sent");
            Attempt::Again
        }
    })
}

/// One queued push, as Redis holds it.
struct Fields {
    identity: String,
    device: String,
    payload: String,
    urgency: String,
    ttl: i64,
    topic: String,
    attempts: i64,
    first: i64,
}

/// Attempts every push due by `now`, rescheduling each the push service
/// could not take with a backoff that doubles, up to five minutes, and
/// dropping one that has failed for an hour. Returns how many were
/// attempted. Servers sharing a Redis can all run this: removing a push
/// from the queue claims it.
pub(crate) async fn deliver_due(app: &App, now: i64) -> Result<usize, Failure> {
    let mut connection = app.store.connection().await?;
    let due: Vec<String> = connection
        .zrangebyscore(app.store.key("pushes"), "-inf", now)
        .await?;
    let mut attempted = 0;
    for id in due {
        let claimed: i64 = connection.zrem(app.store.key("pushes"), &id).await?;
        let key = app.store.key(&format!("push:{id}"));
        let stored: std::collections::HashMap<String, String> = connection.hgetall(&key).await?;
        let (1, Some(fields)) = (claimed, fields(&stored)) else {
            continue;
        };
        attempted += 1;
        match attempt(app, &fields, now).await? {
            Attempt::Again if now - fields.first < GIVE_UP_AFTER => {
                let backoff = (1000_i64 << fields.attempts.min(22)).min(LONGEST_BACKOFF);
                let () = connection
                    .hset(&key, "attempts", fields.attempts + 1)
                    .await?;
                let () = connection
                    .zadd(app.store.key("pushes"), &id, now + backoff)
                    .await?;
            }
            _ => {
                let () = connection.del(&key).await?;
            }
        }
    }
    Ok(attempted)
}

fn fields(stored: &std::collections::HashMap<String, String>) -> Option<Fields> {
    let text = |name: &str| stored.get(name).cloned();
    let number = |name: &str| stored.get(name)?.parse().ok();
    Some(Fields {
        identity: text("identity")?,
        device: text("device")?,
        payload: text("payload")?,
        urgency: text("urgency")?,
        ttl: number("ttl")?,
        topic: text("topic")?,
        attempts: number("attempts")?,
        first: number("first")?,
    })
}
