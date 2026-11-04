//! Sending: a POST to a name creates an exchange with that identity.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, RawQuery, State};
use axum::http::header::LOCATION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

use super::changes::{self, exchange_channel};
use super::records::{self, Exchange, Found, State as ExchangeState};
use super::{consent, reading, view, waiting};
use crate::app::{App, Failure};
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
    let created = records::now();
    let ttl = incoming.message.ttl;
    let admitted = consent::admits(&app, &recipient, &sender.id).await?;
    let exchange = Exchange {
        id: ulid::Ulid::generate().to_string(),
        sender: sender.id.clone(),
        recipient: recipient.id.clone(),
        state: if admitted {
            ExchangeState::Open
        } else {
            ExchangeState::Pending
        },
        message: incoming.message,
        reply: None,
        created,
        expires: ttl
            .map(|ttl| created + i64::try_from(ttl.saturating_mul(1000)).unwrap_or(i64::MAX)),
        purpose: None,
    };
    records::create(&app.store, &exchange).await?;
    changes::publish(&app.store, &exchange).await?;
    if !admitted {
        consent::ask(&app, &recipient, &exchange).await?;
    }
    let location = format!("/exchanges/{}", exchange.id);
    let Some(wait) = wait else {
        let body = Json(view(&app, &exchange, &sender.id).await?);
        return Ok((StatusCode::CREATED, [(LOCATION, location)], body).into_response());
    };
    let (store, id) = (&app.store, exchange.id.as_str());
    let ended = waiting::until(store, &[exchange_channel(id)], wait, move || async move {
        Ok(match records::load(store, id).await? {
            Found::Exchange(exchange) if exchange.state.is_final() => Some(*exchange),
            _ => None,
        })
    })
    .await?;
    if let Some(exchange) = ended {
        return Ok(Json(view(&app, &exchange, &sender.id).await?).into_response());
    }
    let body = Json(view(&app, &exchange, &sender.id).await?);
    Ok((StatusCode::ACCEPTED, [(LOCATION, location)], body).into_response())
}
