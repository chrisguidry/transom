//! Delivering webhooks: a queue in Redis that every server drains, with
//! retries, so a receiver that is down for a while still gets each event.
//!
//! - `deliveries`, a sorted set of delivery ids by when each is next due,
//!   or while a server attempts it, when its lease runs out (`queue.rs`).
//! - `delivery:{id}`, a hash of one delivery's `url`, `body`, how many
//!   `attempts` it has had, and when it was `first` queued.

use std::collections::HashMap;
use std::time::Duration;

use super::signing::{Parameter, base, content_digest, parameters};
use crate::app::{App, Failure};
use crate::queue::Queue;

/// How long transom keeps trying a delivery before giving up on it.
const GIVE_UP_AFTER: i64 = 24 * 60 * 60 * 1000;

/// The longest wait between two attempts.
const LONGEST_BACKOFF: i64 = 60 * 60 * 1000;

/// How long a signature is good for, which bounds how late a captured
/// webhook can be replayed.
const SIGNATURE_LIFETIME: i64 = 5 * 60;

/// How long a receiver has to answer one attempt.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The deliveries waiting for an attempt. An attempt waits up to
/// [`TIMEOUT`] for the receiver, and a lease of a minute leaves room for
/// the signing and the writes around it.
pub(super) const DELIVERIES: Queue = Queue::new("deliveries", "delivery", Duration::from_secs(60));

/// Queues `body` to be sent to `url`, due at once, in `pipe`.
pub(crate) fn enqueue(pipe: &mut redis::Pipeline, app: &App, url: &str, body: &str, now: i64) {
    let id = ulid::Ulid::generate().to_string();
    DELIVERIES.add(
        pipe,
        &app.store,
        &id,
        &[
            ("url", url),
            ("body", body),
            ("attempts", "0"),
            ("first", &now.to_string()),
        ],
        now,
    );
}

/// The headers that sign a POST of `body` to `url`, at `now` in seconds.
pub(crate) async fn signed_headers(
    app: &App,
    url: &str,
    body: &str,
    now: i64,
) -> Result<Vec<(String, String)>, Failure> {
    let signer = app.webhooks.signer(app).await?;
    let digest = content_digest(body.as_bytes());
    let names = ["@method", "@target-uri", "content-type", "content-digest"];
    let values = ["POST", url, "application/json", digest.as_str()];
    let parameters = parameters(
        &names,
        &[
            ("created", Parameter::Integer(now)),
            ("expires", Parameter::Integer(now + SIGNATURE_LIFETIME)),
            ("keyid", Parameter::String(signer.kid.clone())),
            ("alg", Parameter::String("ed25519".to_owned())),
        ],
    );
    let components: Vec<(&str, &str)> = names.into_iter().zip(values).collect();
    let signature = signer.sign(&base(&components, &parameters));
    Ok(vec![
        ("content-type".to_owned(), "application/json".to_owned()),
        ("content-digest".to_owned(), digest),
        (
            "signature-input".to_owned(),
            format!("transom={parameters}"),
        ),
        ("signature".to_owned(), format!("transom=:{signature}:")),
    ])
}

/// Makes one attempt at a delivery, and says whether the receiver took it.
async fn attempt(app: &App, url: &str, body: &str, now: i64) -> Result<bool, Failure> {
    let mut request = app.http.post(url).timeout(TIMEOUT).body(body.to_owned());
    for (name, value) in signed_headers(app, url, body, now / 1000).await? {
        request = request.header(name, value);
    }
    Ok(match request.send().await {
        Ok(response) if response.status().is_success() => {
            tracing::info!(%url, status = %response.status(), "a webhook was delivered");
            true
        }
        Ok(response) => {
            tracing::warn!(%url, status = %response.status(), "a webhook was refused");
            false
        }
        Err(error) => {
            tracing::warn!(%url, %error, "a webhook could not be sent");
            false
        }
    })
}

/// One queued delivery, as Redis holds it.
struct Delivery {
    url: String,
    body: String,
    attempts: i64,
    first: i64,
}

impl Delivery {
    fn read(stored: &HashMap<String, String>) -> Option<Self> {
        let number = |name: &str| stored.get(name)?.parse().ok();
        Some(Self {
            url: stored.get("url")?.clone(),
            body: stored.get("body")?.clone(),
            attempts: number("attempts")?,
            first: number("first")?,
        })
    }
}

/// Attempts every delivery due by `now`, rescheduling each that fails with
/// a backoff that doubles, up to an hour, and dropping one that has failed
/// for a day. Returns how many were delivered. Servers sharing a Redis can
/// all run this: each claims a delivery before it attempts it.
pub(crate) async fn deliver_due(app: &App, now: i64) -> Result<usize, Failure> {
    let store = &app.store;
    let mut delivered = 0;
    for id in DELIVERIES.due(store, now).await? {
        if !DELIVERIES.claim(app, &id, now).await? {
            continue;
        }
        let Some(delivery) = Delivery::read(&DELIVERIES.fields(store, &id).await?) else {
            DELIVERIES.finish(store, &id).await?;
            continue;
        };
        if attempt(app, &delivery.url, &delivery.body, now).await? {
            DELIVERIES.finish(store, &id).await?;
            delivered += 1;
        } else if now - delivery.first >= GIVE_UP_AFTER {
            tracing::warn!(url = %delivery.url, "a webhook failed for a day and was dropped");
            DELIVERIES.finish(store, &id).await?;
        } else {
            let backoff = (1000_i64 << delivery.attempts.min(22)).min(LONGEST_BACKOFF);
            DELIVERIES
                .retry(store, &id, delivery.attempts + 1, now + backoff)
                .await?;
        }
    }
    Ok(delivered)
}
