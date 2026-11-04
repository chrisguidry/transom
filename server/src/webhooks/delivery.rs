//! Delivering webhooks: a queue in Redis that every server drains, with
//! retries, so a receiver that is down for a while still gets each event.
//!
//! - `deliveries`, a sorted set of delivery ids by when each is next due.
//! - `delivery:{id}`, a hash of one delivery's `url`, `body`, how many
//!   `attempts` it has had, and when it was `first` queued.

use std::time::Duration;

use redis::AsyncCommands;

use super::signing::{Parameter, base, content_digest, parameters};
use crate::app::{App, Failure};

/// How long transom keeps trying a delivery before giving up on it.
const GIVE_UP_AFTER: i64 = 24 * 60 * 60 * 1000;

/// The longest wait between two attempts.
const LONGEST_BACKOFF: i64 = 60 * 60 * 1000;

/// How long a signature is good for, which bounds how late a captured
/// webhook can be replayed.
const SIGNATURE_LIFETIME: i64 = 5 * 60;

/// How long a receiver has to answer one attempt.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Queues `body` to be sent to `url`, at once.
pub(crate) async fn enqueue(app: &App, url: &str, body: &str, now: i64) -> Result<(), Failure> {
    let id = ulid::Ulid::generate().to_string();
    let mut connection = app.store.connection().await?;
    let mut pipe = redis::pipe();
    pipe.atomic()
        .hset_multiple(
            app.store.key(&format!("delivery:{id}")),
            &[
                ("url", url),
                ("body", body),
                ("attempts", "0"),
                ("first", &now.to_string()),
            ],
        )
        .zadd(app.store.key("deliveries"), &id, now);
    pipe.query_async::<()>(&mut connection).await?;
    Ok(())
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

/// Attempts every delivery due by `now`, rescheduling each that fails with
/// a backoff that doubles, up to an hour, and dropping one that has failed
/// for a day. Returns how many were delivered. Servers sharing a Redis can
/// all run this: removing a delivery from the queue claims it.
pub(crate) async fn deliver_due(app: &App, now: i64) -> Result<usize, Failure> {
    let mut connection = app.store.connection().await?;
    let due: Vec<String> = connection
        .zrangebyscore(app.store.key("deliveries"), "-inf", now)
        .await?;
    let mut delivered = 0;
    for id in due {
        let claimed: i64 = connection.zrem(app.store.key("deliveries"), &id).await?;
        let key = app.store.key(&format!("delivery:{id}"));
        let (url, body, attempts, first): (
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<i64>,
        ) = redis::cmd("HMGET")
            .arg(&key)
            .arg("url")
            .arg("body")
            .arg("attempts")
            .arg("first")
            .query_async(&mut connection)
            .await?;
        let (1, Some(url), Some(body), Some(attempts), Some(first)) =
            (claimed, url, body, attempts, first)
        else {
            continue;
        };
        if attempt(app, &url, &body, now).await? {
            let () = connection.del(&key).await?;
            delivered += 1;
        } else if now - first >= GIVE_UP_AFTER {
            tracing::warn!(%url, "a webhook failed for a day and was dropped");
            let () = connection.del(&key).await?;
        } else {
            let backoff = (1000_i64 << attempts.min(22)).min(LONGEST_BACKOFF);
            let () = connection.hset(&key, "attempts", attempts + 1).await?;
            let () = connection
                .zadd(app.store.key("deliveries"), &id, now + backoff)
                .await?;
        }
    }
    Ok(delivered)
}
