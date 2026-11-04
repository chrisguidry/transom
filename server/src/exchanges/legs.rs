//! An exchange and its legs: the whole exchange, the message, the reply,
//! and each recipient's private read state.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::Event;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde_json::json;

use super::changes::{self, Replying, exchange_channel, identity_channel};
use super::records::{self, Exchange, Reply, State as ExchangeState};
use super::waiting::{self, Step};
use super::{is_recipient, reply_view, view, visible};
use crate::app::{App, Failure};
use crate::identities::caller;
use crate::signin::Verified;

type Outcome = Result<Response, Failure>;

pub(super) async fn show(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    verified: Verified,
) -> Outcome {
    let me = caller(&app, &verified).await?;
    let viewer = me.id.clone();
    let exchange = visible(&app, &id, &viewer).await?;
    if crate::web::wants_html(&headers) {
        let role = crate::web::pages::Role {
            recipient: is_recipient(&app, &exchange, &viewer).await?,
            sender: exchange.sender == viewer,
        };
        return crate::web::pages::exchange(
            &me.name,
            &view(&app, &exchange, &viewer).await?,
            &role,
        );
    }
    if !waiting::wants_events(&headers) {
        return Ok(Json(view(&app, &exchange, &viewer).await?).into_response());
    }
    let store_app = app.clone();
    // `?changes` leaves out the exchange as it stands when the stream
    // opens. A page that re-renders on each event, and reconnects as it
    // does, would otherwise re-render on its own snapshot without end.
    let changes_only = query
        .as_deref()
        .is_some_and(|query| query.split('&').any(|pair| pair == "changes"));
    let sse = waiting::stream(&app.store, &[exchange_channel(&id)], move |payload| {
        let (app, id, viewer) = (store_app.clone(), id.clone(), viewer.clone());
        async move {
            let exchange = visible(&app, &id, &viewer).await?;
            let done = exchange.state.is_final();
            if changes_only && payload.is_none() {
                return Ok(Step {
                    events: vec![],
                    done,
                });
            }
            let event = Event::default()
                .event("exchange")
                .json_data(view(&app, &exchange, &viewer).await?);
            Ok(Step {
                events: vec![event.expect("a view serializes")],
                done,
            })
        }
    })
    .await?;
    Ok(sse.into_response())
}

pub(super) async fn message(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    verified: Verified,
) -> Outcome {
    let viewer = caller(&app, &verified).await?.id;
    let exchange = visible(&app, &id, &viewer).await?;
    let status = if exchange.state == ExchangeState::Withdrawn {
        StatusCode::GONE
    } else {
        StatusCode::OK
    };
    Ok((status, Json(super::message_view(&exchange, &viewer))).into_response())
}

pub(super) async fn withdraw(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    verified: Verified,
) -> Outcome {
    let viewer = caller(&app, &verified).await?.id;
    let exchange = withdraw_as(&app, &viewer, &id).await?;
    Ok(Json(view(&app, &exchange, &viewer).await?).into_response())
}

/// Withdraws the message of exchange `id`, as its sender `viewer`.
/// Withdrawing again changes nothing.
pub(crate) async fn withdraw_as(app: &App, viewer: &str, id: &str) -> Result<Exchange, Failure> {
    let exchange = visible(app, id, viewer).await?;
    if exchange.sender != viewer {
        return Err(Failure::new(
            StatusCode::FORBIDDEN,
            "only the sender may withdraw a message",
        ));
    }
    let (moved, was) = changes::transition(
        &app.store,
        id,
        &[ExchangeState::Open, ExchangeState::Pending],
        ExchangeState::Withdrawn,
    )
    .await?;
    if !moved && was != Some(ExchangeState::Withdrawn) {
        let was = was.map_or("gone", ExchangeState::as_str);
        return Err(Failure::new(
            StatusCode::CONFLICT,
            format!("the exchange is already {was}"),
        ));
    }
    let exchange = visible(app, id, viewer).await?;
    if moved {
        super::announce(app, &exchange).await?;
    }
    Ok(exchange)
}

/// The reply, once there is one, or what ended the exchange without one.
async fn reply_or_end(
    app: &App,
    id: &str,
    viewer: &str,
) -> Result<Option<Result<Reply, ExchangeState>>, Failure> {
    let exchange = visible(app, id, viewer).await?;
    Ok(match exchange.reply {
        Some(reply) => Some(Ok(reply)),
        None if exchange.state.is_final() => Some(Err(exchange.state)),
        None => None,
    })
}

pub(super) async fn reply(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    verified: Verified,
) -> Outcome {
    let viewer = caller(&app, &verified).await?.id;
    visible(&app, &id, &viewer).await?;
    if waiting::wants_events(&headers) {
        let stream_app = app.clone();
        let sse = waiting::stream(&app.store, &[exchange_channel(&id)], move |_| {
            let (app, id, viewer) = (stream_app.clone(), id.clone(), viewer.clone());
            async move {
                let event = match reply_or_end(&app, &id, &viewer).await? {
                    Some(Ok(reply)) => Event::default()
                        .event("reply")
                        .json_data(reply_view(&app, &reply).await?),
                    Some(Err(state)) => Event::default()
                        .event("ended")
                        .json_data(json!({ "state": state.as_str() })),
                    None => {
                        return Ok(Step {
                            events: vec![],
                            done: false,
                        });
                    }
                };
                Ok(Step {
                    events: vec![event.expect("a reply serializes")],
                    done: true,
                })
            }
        })
        .await?;
        return Ok(sse.into_response());
    }
    let wait = waiting::wait(&headers, query.as_deref())?.unwrap_or_default();
    let (waiting_app, waiting_id, waiting_viewer) = (&*app, id.as_str(), viewer.as_str());
    let found = waiting::until(
        &app.store,
        &[exchange_channel(&id)],
        wait,
        move || async move { reply_or_end(waiting_app, waiting_id, waiting_viewer).await },
    )
    .await?;
    match found {
        Some(Ok(reply)) => Ok(Json(reply_view(&app, &reply).await?).into_response()),
        Some(Err(state)) => Err(Failure::new(
            StatusCode::NOT_FOUND,
            format!("the exchange ended {} without a reply", state.as_str()),
        )),
        None => Err(Failure::new(StatusCode::NOT_FOUND, "there is no reply yet")),
    }
}

/// The choice a reply names, as its key, if the message offers it.
fn chosen(exchange: &Exchange, choice: Option<String>) -> Result<Option<String>, Failure> {
    let Some(choice) = choice else {
        return Ok(None);
    };
    exchange
        .message
        .choices
        .iter()
        .find(|offered| offered.key() == choice || offered.label == choice)
        .map(|offered| Some(offered.key().to_owned()))
        .ok_or_else(|| {
            Failure::new(
                StatusCode::BAD_REQUEST,
                format!("{choice:?} is not one of the choices"),
            )
        })
}

pub(super) async fn put_reply(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    verified: Verified,
    body: Bytes,
) -> Outcome {
    let viewer = caller(&app, &verified).await?.id;
    let (choice, text) = super::reading::read_reply(&headers, &body)
        .map_err(|reason| Failure::new(StatusCode::BAD_REQUEST, reason))?;
    let strict = headers
        .get("if-none-match")
        .is_some_and(|value| value.as_bytes() == b"*");
    let exchange = reply_as(&app, &viewer, &id, choice, text, strict).await?;
    Ok(Json(view(&app, &exchange, &viewer).await?).into_response())
}

/// Replies to exchange `id` as its recipient `viewer`. Repeating the same
/// reply changes nothing; a different reply after the first is refused,
/// with the first, and so is any reply after the first when `strict`.
pub(crate) async fn reply_as(
    app: &App,
    viewer: &str,
    id: &str,
    choice: Option<String>,
    text: Option<String>,
    strict: bool,
) -> Result<Exchange, Failure> {
    let exchange = visible(app, id, viewer).await?;
    if !is_recipient(app, &exchange, viewer).await? {
        return Err(Failure::new(
            StatusCode::FORBIDDEN,
            "only a recipient may reply",
        ));
    }
    let choice = chosen(&exchange, choice)?;
    if exchange.purpose.is_some() && choice.is_none() {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "this asks for a decision: reply with one of its choices",
        ));
    }
    if choice.is_none() && text.is_none() {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "a reply needs a choice or text",
        ));
    }
    if strict && exchange.reply.is_some() {
        return Err(Failure::new(
            StatusCode::PRECONDITION_FAILED,
            "the exchange already has a reply",
        ));
    }
    let reply = Reply {
        choice,
        text,
        by: viewer.to_owned(),
        at: records::now(),
    };
    match changes::reply(&app.store, id, &reply).await? {
        Replying::Accepted => {
            let exchange = visible(app, id, viewer).await?;
            if let Some(choice) = &reply.choice {
                super::consent::settle(app, &exchange, choice).await?;
            }
            records::set_read(&app.store, viewer, id, true).await?;
            super::announce(app, &exchange).await?;
            Ok(exchange)
        }
        Replying::Refused(_, Some(existing))
            if !strict
                && existing.by == reply.by
                && existing.choice == reply.choice
                && existing.text == reply.text =>
        {
            visible(app, id, viewer).await
        }
        Replying::Refused(ExchangeState::Replied, Some(existing)) => Err(Failure::new(
            StatusCode::CONFLICT,
            "the exchange already has a reply",
        )
        .with("reply", reply_view(app, &existing).await?)),
        Replying::Refused(ExchangeState::Pending, _) => Err(Failure::new(
            StatusCode::CONFLICT,
            "the exchange is waiting for the recipient's consent",
        )),
        Replying::Refused(state, _) => Err(Failure::new(
            StatusCode::CONFLICT,
            format!("the exchange is {}", state.as_str()),
        )),
    }
}

/// The identity named by `target` in `/exchanges/{id}/@{name}`, which must
/// be the caller: read state is private to each recipient.
async fn reader(app: &App, id: &str, target: &str, verified: &Verified) -> Result<String, Failure> {
    let caller = caller(app, verified).await?;
    let name = target
        .strip_prefix('@')
        .ok_or_else(|| Failure::new(StatusCode::NOT_FOUND, "not found"))?;
    if name != caller.name {
        return Err(Failure::new(
            StatusCode::FORBIDDEN,
            "read state is private to each recipient",
        ));
    }
    let exchange = visible(app, id, &caller.id).await?;
    if !is_recipient(app, &exchange, &caller.id).await? {
        return Err(Failure::new(
            StatusCode::NOT_FOUND,
            "only a recipient has read state",
        ));
    }
    Ok(caller.id)
}

pub(super) async fn read_state(
    State(app): State<Arc<App>>,
    Path((id, target)): Path<(String, String)>,
    verified: Verified,
) -> Outcome {
    let reader = reader(&app, &id, &target, &verified).await?;
    let read = records::read_ids(&app.store, &reader).await?.contains(&id);
    Ok(Json(json!({ "read": read })).into_response())
}

async fn set_read(app: &App, id: &str, target: &str, verified: &Verified, read: bool) -> Outcome {
    let reader = reader(app, id, target, verified).await?;
    records::set_read(&app.store, &reader, id, read).await?;
    let mut connection = app.store.connection().await?;
    redis::cmd("PUBLISH")
        .arg(app.store.key(&identity_channel(&reader)))
        .arg(id)
        .query_async::<()>(&mut connection)
        .await?;
    Ok(Json(json!({ "read": read })).into_response())
}

pub(super) async fn mark_read(
    State(app): State<Arc<App>>,
    Path((id, target)): Path<(String, String)>,
    verified: Verified,
) -> Outcome {
    set_read(&app, &id, &target, &verified, true).await
}

pub(super) async fn mark_unread(
    State(app): State<Arc<App>>,
    Path((id, target)): Path<(String, String)>,
    verified: Verified,
) -> Outcome {
    set_read(&app, &id, &target, &verified, false).await
}
