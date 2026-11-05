//! Sending: a POST to a name creates an exchange with that identity.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, RawQuery, State};
use axum::http::header::LOCATION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

use super::attachments::{self, Attachment};
use super::changes::exchange_channel;
use super::message::Message;
use super::records::{self, Exchange, Found, State as ExchangeState};
use super::{consent, reading, view, waiting};
use crate::app::{App, Failure};
use crate::identities::records::Identity;
use crate::identities::{self, caller};
use crate::signin::Verified;

pub(crate) async fn send(
    State(app): State<Arc<App>>,
    Path(path): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    verified: Verified,
    body: Bytes,
) -> Result<Response, Failure> {
    let recipient = match identities::resolve(&app, &path).await? {
        identities::Resolved::Current(identity) => identity,
        identities::Resolved::Moved(response) => return Ok(response),
    };
    let sender = caller(&app, &verified).await?;
    let wait = waiting::wait(&headers, query.as_deref())?;
    let incoming = reading::read(&headers, query.as_deref(), body)
        .await
        .map_err(|reason| Failure::new(StatusCode::BAD_REQUEST, reason))?;
    let exchange = deliver(
        &app,
        &sender,
        &recipient,
        incoming.message,
        incoming.attachments,
    )
    .await?;
    let location = format!("/exchanges/{}", exchange.id);
    let Some(wait) = wait else {
        let body = Json(view(&app, &exchange, &sender.id).await?);
        return Ok((StatusCode::CREATED, [(LOCATION, location)], body).into_response());
    };
    let ended = until_ended(&app, &exchange.id, wait).await?;
    if let Some(exchange) = ended {
        return Ok(Json(view(&app, &exchange, &sender.id).await?).into_response());
    }
    let body = Json(view(&app, &exchange, &sender.id).await?);
    Ok((StatusCode::ACCEPTED, [(LOCATION, location)], body).into_response())
}

/// Sends `message` from `sender` to `recipient`, with its attachments: it
/// opens at once if the recipient admits the sender, replacing the open
/// exchange with the same key, and is held while `@transom` asks if not.
pub(crate) async fn deliver(
    app: &App,
    sender: &Identity,
    recipient: &Identity,
    mut message: Message,
    attachments: Vec<Attachment>,
) -> Result<Exchange, Failure> {
    let (attachments, attached) = attachments::check(attachments)?;
    message.attachments = attached;
    let created = records::now();
    let admitted = consent::admits(app, recipient, &sender.id).await?;
    let mut exchange = Exchange {
        id: records::new_id(),
        sender: sender.id.clone(),
        recipient: recipient.id.clone(),
        state: if admitted {
            ExchangeState::Open
        } else {
            ExchangeState::Pending
        },
        expires: message
            .ttl
            .map(|ttl| created + i64::try_from(ttl.saturating_mul(1000)).unwrap_or(i64::MAX)),
        message,
        reply: None,
        created,
        purpose: None,
        replaces: None,
        repeats: 0,
    };
    attachments::store(app, &exchange.id, &attachments).await?;
    records::create(&app.store, &exchange).await?;
    super::replacing::replace(app, &mut exchange).await?;
    super::announce(app, &exchange).await?;
    if !admitted {
        consent::ask(app, recipient, &exchange).await?;
    }
    Ok(exchange)
}

/// The exchange once it ends, if it ends within `wait`.
pub(crate) async fn until_ended(
    app: &App,
    id: &str,
    wait: std::time::Duration,
) -> Result<Option<Exchange>, Failure> {
    let store = &app.store;
    waiting::until(store, &[exchange_channel(id)], wait, move || async move {
        Ok(match records::load(store, id).await? {
            Found::Exchange(exchange) if exchange.state.is_final() => Some(*exchange),
            _ => None,
        })
    })
    .await
}
