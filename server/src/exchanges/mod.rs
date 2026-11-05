//! Exchanges: a message and, if there is one, its reply. An exchange lives
//! at `/exchanges/{id}`, and each of its legs has a URL of its own.

pub(crate) mod attachments;
mod changes;
mod consent;
mod expiry;
mod feed;
pub(crate) mod fields;
pub(crate) mod legs;
mod links;
pub(crate) mod listing;
pub(crate) mod message;
mod reading;
pub(crate) mod records;
mod repeats;
mod replacing;
pub(crate) mod retention;
pub(crate) mod sending;
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
pub(crate) use repeats::repeat_due;
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
            "/exchanges/{id}/attachments/{name}",
            get(attachments::serve),
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
pub(crate) async fn visible(app: &App, id: &str, viewer: &str) -> Result<Exchange, Failure> {
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
    let attachments = json!(message.attachments);
    let mut view = serde_json::to_value(message).expect("a message serializes");
    // The stored record leaves out an empty list, but a client reads the
    // list on every message, so the view always has one.
    view["attachments"] = attachments;
    view
}

/// The whole exchange as JSON, as `viewer` sees it.
pub(crate) async fn view(app: &App, exchange: &Exchange, viewer: &str) -> Result<Value, Failure> {
    let reply = match &exchange.reply {
        Some(reply) => reply_view(app, reply).await?,
        None => Value::Null,
    };
    let mut view = json!({
        "id": exchange.id,
        // Absolute when transom knows its public URL, so a webhook's
        // receiver or an agent can follow it from anywhere.
        "url": format!(
            "{}/exchanges/{}",
            app.public_url.as_deref().unwrap_or(""),
            exchange.id
        ),
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
    if let Some(replaces) = &exchange.replaces {
        view["replaces"] = json!(replaces);
    }
    if exchange.repeats > 0 {
        view["repeats"] = json!(exchange.repeats);
    }
    let url = view["url"].as_str().unwrap_or_default().to_owned();
    for attachment in view["message"]["attachments"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        let name = attachment["name"].as_str().unwrap_or_default().to_owned();
        attachment["url"] = json!(format!("{url}/attachments/{name}"));
    }
    Ok(view)
}

/// Deletes every exchange a deleted group received.
pub(crate) async fn bury(app: &App, group: &str) -> Result<(), Failure> {
    Ok(changes::bury(&app.store, group).await?)
}

/// Tells `individual`'s live feeds that it joined or left a group, so each
/// ends and its client reconnects to the groups it has now.
pub(crate) async fn announce_groups(app: &App, individual: &str) -> Result<(), Failure> {
    Ok(changes::publish_groups(&app.store, individual).await?)
}

/// Announces a change to `exchange`: to anything waiting on it or
/// following a feed it is in, and to the webhooks of those it happened to.
/// An exchange that opened is `received` by its recipient, and by each
/// member when the recipient is a group. One that ended has `ended` for its
/// sender. A change that starts the exchange aging schedules its deletion;
/// a server that stops between the change and this schedules it with the
/// backfill at its next start. An urgent exchange that opened schedules
/// its repeats.
pub(crate) async fn announce(app: &App, exchange: &Exchange) -> Result<(), Failure> {
    changes::publish(&app.store, exchange).await?;
    retention::schedule(app, exchange, records::now()).await?;
    repeats::schedule(app, exchange, records::now()).await?;
    crate::push::announce(app, exchange).await?;
    let (event, to) = if exchange.state == records::State::Open {
        let mut to = vec![exchange.recipient.clone()];
        to.extend(identities::members(&app.store, &exchange.recipient).await?);
        ("received", to)
    } else if exchange.state.is_final() {
        ("ended", vec![exchange.sender.clone()])
    } else {
        return Ok(());
    };
    for identity in to {
        if let Some(url) = identities::webhook(&app.store, &identity).await? {
            let body = json!({ "event": event, "exchange": view(app, exchange, &identity).await? });
            crate::webhooks::enqueue(app, &url, &body.to_string(), records::now()).await?;
        }
    }
    Ok(())
}
