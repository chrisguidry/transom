//! Push notifications to browsers, with Web Push (RFC 8030, encrypted per
//! RFC 8291 and signed per RFC 8292). A browser subscribes at
//! `/@{name}/devices`. Each exchange that arrives for that identity, or
//! for a group it is in, reaches the browser as a notification, and a
//! second push clears it when the exchange ends anywhere.

mod delivery;
pub(crate) mod devices;
pub(crate) mod encryption;
#[cfg(test)]
mod tests;
mod vapid;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, put};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use redis::AsyncCommands;
use serde_json::{Value, json};
use tokio::sync::OnceCell;

use crate::app::{App, Failure};
use crate::exchanges::records::{self, Exchange, State};
use crate::identities::records as identities;
use delivery::Push;
pub(crate) use delivery::deliver_due;
use vapid::Vapid;

/// How long a push service keeps an arrival for a browser that is offline,
/// when the exchange sets no time to live: four weeks, the most push
/// services keep anything.
const KEEP: i64 = 28 * 24 * 60 * 60;

/// How long a push service keeps the push that clears a notification.
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
/// sender's.
pub(crate) async fn announce(app: &App, exchange: &Exchange) -> Result<(), Failure> {
    let now = records::now();
    let (payload, ttl) = if exchange.state == State::Open {
        let ttl = exchange
            .expires
            .map_or(KEEP, |expires| ((expires - now) / 1000).max(0));
        (arrival(app, exchange).await?, ttl)
    } else if exchange.state.is_final() {
        (json!({ "event": "ended", "id": exchange.id }), KEEP_ENDED)
    } else {
        return Ok(());
    };
    let payload = payload.to_string();
    let urgency = serde_json::to_value(exchange.message.urgency).expect("an urgency serializes");
    let push = Push {
        payload: &payload,
        urgency: urgency.as_str().unwrap_or("normal"),
        ttl,
        topic: &exchange.id,
    };
    let mut to = vec![exchange.recipient.clone()];
    to.extend(identities::members(&app.store, &exchange.recipient).await?);
    for identity in to.iter().filter(|identity| **identity != exchange.sender) {
        delivery::enqueue(app, identity, &push, now).await?;
    }
    Ok(())
}

/// What a notification of an arrival shows: the summary, who sent it and
/// the start of the body, and the first choices as buttons.
async fn arrival(app: &App, exchange: &Exchange) -> Result<Value, Failure> {
    let message = &exchange.message;
    let from = identities::identity(&app.store, &exchange.sender)
        .await?
        .name;
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
    Ok(json!({
        "event": "received",
        "id": exchange.id,
        "url": format!("{}/exchanges/{}", app.public_url.as_deref().unwrap_or(""), exchange.id),
        "title": clip(title, SUMMARY),
        "body": clip(&format!("@{from}: {body}"), BODY),
        "choices": choices,
    }))
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
