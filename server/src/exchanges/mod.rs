//! Exchanges: a message and, if there is one, its reply. An exchange lives
//! at `/exchanges/{id}`, and each of its legs has a URL of its own.

mod changes;
mod consent;
mod expiry;
mod fields;
mod legs;
mod links;
mod listing;
pub(crate) mod message;
mod reading;
mod records;
mod sending;
#[cfg(test)]
mod tests;
mod waiting;

use std::sync::Arc;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use serde_json::{Value, json};

use crate::app::{App, Failure};
use crate::identities::records as identities;
pub(crate) use consent::invite;
pub(crate) use expiry::run as expire;
pub(crate) use listing::list;
use records::{Exchange, Found};
pub(crate) use sending::send;

pub(crate) fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/exchanges/{id}", get(legs::show))
        .route(
            "/exchanges/{id}/message",
            get(legs::message).delete(legs::withdraw),
        )
        .route(
            "/exchanges/{id}/reply",
            get(legs::reply).put(legs::put_reply),
        )
        .route(
            "/exchanges/{id}/{target}",
            get(legs::read_state)
                .put(legs::mark_read)
                .delete(legs::mark_unread),
        )
}

/// Whether `viewer` is one of the exchange's recipients: the individual it
/// went to, or a member of the group it went to.
async fn is_recipient(app: &App, exchange: &Exchange, viewer: &str) -> Result<bool, Failure> {
    Ok(exchange.recipient == viewer
        || identities::is_member(&app.store, &exchange.recipient, viewer).await?)
}

/// The exchange at `id`, if `viewer` may see it: its sender or one of its
/// recipients. Anyone else gets the same `404` as for no exchange at all,
/// so an id reveals nothing to someone it was not meant for.
async fn visible(app: &App, id: &str, viewer: &str) -> Result<Exchange, Failure> {
    let not_found = || Failure::new(StatusCode::NOT_FOUND, format!("no exchange {id}"));
    match records::load(&app.store, id).await? {
        Found::Exchange(exchange)
            if exchange.sender == viewer || is_recipient(app, &exchange, viewer).await? =>
        {
            Ok(*exchange)
        }
        Found::Gone => Err(Failure::new(
            StatusCode::GONE,
            format!("exchange {id} was deleted"),
        )),
        Found::Exchange(_) | Found::Missing => Err(not_found()),
    }
}

fn timestamp(milliseconds: i64) -> String {
    jiff::Timestamp::from_millisecond(milliseconds)
        .map(|timestamp| timestamp.to_string())
        .unwrap_or_default()
}

async fn name(app: &App, id: &str) -> Result<String, Failure> {
    Ok(format!(
        "@{}",
        identities::identity(&app.store, id).await?.name
    ))
}

/// The reply as JSON, naming who replied.
async fn reply_view(app: &App, reply: &records::Reply) -> Result<Value, Failure> {
    let mut view = json!({ "by": name(app, &reply.by).await?, "at": timestamp(reply.at) });
    if let Some(choice) = &reply.choice {
        view["choice"] = json!(choice);
    }
    if let Some(text) = &reply.text {
        view["text"] = json!(text);
    }
    Ok(view)
}

/// The message as `viewer` sees it: only its sender sees the metadata.
fn message_view(exchange: &Exchange, viewer: &str) -> Value {
    let message = if exchange.sender == viewer {
        exchange.message.clone()
    } else {
        exchange.message.for_recipient()
    };
    serde_json::to_value(message).expect("a message serializes")
}

/// The whole exchange as JSON, as `viewer` sees it.
async fn view(app: &App, exchange: &Exchange, viewer: &str) -> Result<Value, Failure> {
    let reply = match &exchange.reply {
        Some(reply) => reply_view(app, reply).await?,
        None => Value::Null,
    };
    let mut view = json!({
        "id": exchange.id,
        "url": format!("/exchanges/{}", exchange.id),
        "state": exchange.state.as_str(),
        "question": exchange.message.is_question(),
        "from": name(app, &exchange.sender).await?,
        "to": name(app, &exchange.recipient).await?,
        "created": timestamp(exchange.created),
        "message": message_view(exchange, viewer),
        "reply": reply,
    });
    if let Some(expires) = exchange.expires {
        view["expires"] = json!(timestamp(expires));
    }
    Ok(view)
}

/// Deletes every exchange a deleted group received.
pub(crate) async fn bury(app: &App, group: &str) -> Result<(), Failure> {
    Ok(changes::bury(&app.store, group).await?)
}
