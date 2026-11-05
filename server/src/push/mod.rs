//! Push notifications to browsers, with Web Push (RFC 8030, encrypted per
//! RFC 8291 and signed per RFC 8292), and wake-ups to phones, through the
//! wake-up gateway and FCM. A device registers at `/@{name}/devices`. Each
//! exchange that arrives for that identity, or for a group it is in,
//! reaches a browser as a notification, and a second push clears it when
//! the exchange ends anywhere, or when the identity reads it on any
//! device. Each of those events wakes a phone, which syncs and shows or
//! clears the notification itself.

mod delivery;
pub(crate) mod devices;
pub(crate) mod encryption;
#[cfg(test)]
mod tests;
mod vapid;
mod wakes;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, put};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use redis::AsyncCommands;
use serde_json::{Value, json};
use tokio::sync::OnceCell;

use crate::app::{App, Failure};
use crate::exchanges::message::Urgency;
use crate::exchanges::records::{self, Exchange, State};
use crate::identities::records as identities;
use delivery::Push;
pub(crate) use delivery::deliver_due;
use vapid::Vapid;
pub(crate) use wakes::wake_due;

/// How long a push service keeps an arrival for a browser that is offline,
/// when the exchange sets no time to live: four weeks, the most push
/// services keep anything.
const KEEP: i64 = 28 * 24 * 60 * 60;

/// How long a push service keeps a push that clears a notification.
const KEEP_ENDED: i64 = 24 * 60 * 60;

/// How much of the summary and the body a notification shows.
const SUMMARY: usize = 120;
const BODY: usize = 300;

/// How many choices a notification offers as buttons: Android shows at
/// most two from a browser.
const BUTTONS: usize = 2;

/// The VAPID key, generated the first time any server needs it and kept
/// in Redis, so every server sharing that Redis pushes with the key the
/// browsers subscribed with.
pub(crate) struct Keys {
    vapid: OnceCell<Arc<Vapid>>,
}

impl Keys {
    pub(crate) fn new() -> Self {
        Self {
            vapid: OnceCell::new(),
        }
    }

    pub(crate) async fn vapid(&self, app: &App) -> Result<Arc<Vapid>, Failure> {
        let vapid = self
            .vapid
            .get_or_try_init(|| async {
                let mut connection = app.store.connection().await?;
                let key = app.store.key("push-key");
                let _: bool = connection
                    .set_nx(&key, STANDARD.encode(Vapid::generate()))
                    .await?;
                let stored: String = connection.get(&key).await?;
                let pkcs8 = STANDARD.decode(stored).unwrap_or_default();
                Vapid::from_pkcs8(&pkcs8).map(Arc::new).map_err(|reason| {
                    Failure::new(axum::http::StatusCode::INTERNAL_SERVER_ERROR, reason)
                })
            })
            .await?;
        Ok(vapid.clone())
    }
}

pub(crate) fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/{target}/devices", get(devices::list))
        .route(
            "/{target}/devices/{device}",
            put(devices::put).delete(devices::remove),
        )
}

/// Pushes an exchange's arrival, or its end, to the devices of the
/// identity it was sent to and of that identity's members, except its
/// sender's, and wakes their phones.
pub(crate) async fn announce(app: &App, exchange: &Exchange) -> Result<(), Failure> {
    let now = records::now();
    let (payload, ttl) = if exchange.state == State::Open {
        (arrival(app, exchange).await?, arrival_ttl(exchange, now))
    } else if exchange.state.is_final() {
        (json!({ "event": "ended", "id": exchange.id }), KEEP_ENDED)
    } else {
        return Ok(());
    };
    let payload = payload.to_string();
    let push = Push {
        payload: &payload,
        urgency: exchange.message.urgency.web_push(),
        ttl,
        topic: &exchange.id,
    };
    let mut to = vec![exchange.recipient.clone()];
    to.extend(identities::members(&app.store, &exchange.recipient).await?);
    for identity in to.iter().filter(|identity| **identity != exchange.sender) {
        notify(app, identity, exchange, &push, now).await?;
    }
    Ok(())
}

/// Shows `exchange` again on `identity`'s devices, as when it arrived, at
/// `now`.
pub(crate) async fn repeat(
    app: &App,
    identity: &str,
    exchange: &Exchange,
    now: i64,
) -> Result<(), Failure> {
    let payload = arrival(app, exchange).await?.to_string();
    let push = Push {
        payload: &payload,
        urgency: exchange.message.urgency.web_push(),
        ttl: arrival_ttl(exchange, now),
        topic: &exchange.id,
    };
    notify(app, identity, exchange, &push, now).await
}

/// How many seconds a push service keeps an arrival: until the exchange
/// expires, or [`KEEP`] when it does not.
fn arrival_ttl(exchange: &Exchange, now: i64) -> i64 {
    exchange
        .expires
        .map_or(KEEP, |expires| ((expires - now) / 1000).max(0))
}

/// Queues `push` about `exchange` for each of `identity`'s browsers, and a
/// wake-up for each of its phones. A browser hears nothing of a very-low
/// exchange: it shows no notification for one, and a browser that takes
/// a push without showing a notification shows one of its own, that the
/// site updated in the background.
async fn notify(
    app: &App,
    identity: &str,
    exchange: &Exchange,
    push: &Push<'_>,
    now: i64,
) -> Result<(), Failure> {
    let devices = devices::of(app, identity).await?;
    if exchange.message.urgency != Urgency::VeryLow {
        delivery::enqueue(app, identity, &devices, push, now).await?;
    }
    wakes::enqueue(app, identity, &devices, now).await
}

/// Pushes to `reader`'s own devices that it read `exchange`, and wakes its
/// phones, so each one clears the notification. No one else's devices hear it, since read
/// state is private. Its `Topic` replaces an arrival a push service still
/// holds for a device that is offline.
pub(crate) async fn read(app: &App, reader: &str, exchange: &Exchange) -> Result<(), Failure> {
    let payload = json!({ "event": "read", "id": exchange.id }).to_string();
    let push = Push {
        payload: &payload,
        urgency: exchange.message.urgency.web_push(),
        ttl: KEEP_ENDED,
        topic: &exchange.id,
    };
    notify(app, reader, exchange, &push, records::now()).await
}

/// What a notification of an arrival shows: the summary, who sent it and
/// the start of the body, the first choices as buttons, and the sender's
/// picture as its icon, when it has one. Its urgency decides how the
/// browser shows it, and its tag is what a later message with the same key
/// replaces: the key, with the sender and the recipient, since another
/// sender's key of the same name is another notification.
async fn arrival(app: &App, exchange: &Exchange) -> Result<Value, Failure> {
    let message = &exchange.message;
    let sender = identities::identity(&app.store, &exchange.sender).await?;
    let from = sender
        .display_name
        .clone()
        .unwrap_or_else(|| format!("@{}", sender.name));
    let body = message.body.as_deref().unwrap_or("");
    let title = message
        .summary
        .as_deref()
        .or_else(|| body.lines().next())
        .filter(|title| !title.is_empty())
        .unwrap_or("A new message");
    let choices: Vec<Value> = message
        .choices
        .iter()
        .take(BUTTONS)
        .map(|choice| json!({ "label": clip(&choice.label, 40), "key": choice.key() }))
        .collect();
    let mut payload = json!({
        "event": "received",
        "id": exchange.id,
        "url": format!("{}/exchanges/{}", app.public_url.as_deref().unwrap_or(""), exchange.id),
        "title": clip(title, SUMMARY),
        "body": clip(&format!("{from}: {body}"), BODY),
        "choices": choices,
        "urgency": message.urgency.as_str(),
        "tag": message.key.as_ref().map_or_else(
            || exchange.id.clone(),
            |key| format!("{}:{}:{key}", exchange.sender, exchange.recipient),
        ),
    });
    if let Some(icon) = crate::identities::pictures::url(app, &sender) {
        payload["icon"] = json!(icon);
    }
    Ok(payload)
}

/// `text`, cut to `most` characters, with an ellipsis when it was cut.
fn clip(text: &str, most: usize) -> String {
    if text.chars().count() <= most {
        return text.to_owned();
    }
    let mut clipped: String = text.chars().take(most - 1).collect();
    clipped.push('…');
    clipped
}
