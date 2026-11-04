//! An identity's exchanges, sent and received, at `/@{name}/exchanges`:
//! the inbox, and as an event stream, the live feed.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::Event;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use super::changes::identity_channel;
use super::records::{self, Found, State as ExchangeState};
use super::waiting::{self, Step};
use super::{is_recipient, view};
use crate::app::{App, Failure};
use crate::identities::records::{self as identities, Identity, Kind};
use crate::identities::{Resolved, caller, resolve};
use crate::signin::Verified;

const DEFAULT_LIMIT: usize = 50;
const MOST: usize = 200;

#[derive(Deserialize)]
pub(crate) struct Filters {
    state: Option<String>,
    read: Option<bool>,
    question: Option<bool>,
    limit: Option<usize>,
}

/// The sorted sets of exchange ids an identity's list draws on: what it
/// sent and received, and for an individual, what its groups received.
async fn indexes(app: &App, identity: &Identity) -> Result<(Vec<String>, Vec<String>), Failure> {
    let mut indexes = vec![
        format!("sent:{}", identity.id),
        format!("received:{}", identity.id),
    ];
    let mut channels = vec![identity_channel(&identity.id)];
    if identity.kind == Kind::Individual {
        for group in identities::groups_of(&app.store, &identity.id).await? {
            indexes.push(format!("received:{group}"));
            channels.push(identity_channel(&group));
        }
    }
    Ok((indexes, channels))
}

pub(crate) async fn list(
    State(app): State<Arc<App>>,
    Path(path): Path<String>,
    Query(filters): Query<Filters>,
    headers: HeaderMap,
    verified: Verified,
) -> Result<Response, Failure> {
    let identity = match resolve(&app, &path).await? {
        Resolved::Current(identity) => identity,
        Resolved::Moved(response) => return Ok(response),
    };
    let viewer = caller(&app, &verified).await?.id;
    let member = identity.kind == Kind::Group
        && identities::is_member(&app.store, &identity.id, &viewer).await?;
    if identity.id != viewer && !member {
        return Err(Failure::new(
            StatusCode::FORBIDDEN,
            "an identity's exchanges are its own",
        ));
    }
    let wanted_state = filters
        .state
        .as_deref()
        .map(|state| {
            ExchangeState::parse(state).ok_or_else(|| {
                Failure::new(StatusCode::BAD_REQUEST, format!("{state:?} is not a state"))
            })
        })
        .transpose()?;
    let (indexes, channels) = indexes(&app, &identity).await?;
    if waiting::wants_events(&headers) {
        let stream_app = app.clone();
        let sse = waiting::stream(&app.store, &channels, move |payload| {
            let (app, viewer) = (stream_app.clone(), viewer.clone());
            async move {
                let Some(id) = payload else {
                    return Ok(Step {
                        events: vec![],
                        done: false,
                    });
                };
                // An exchange of a deleted group is gone, and the feed says
                // so, for a client to drop it.
                let Found::Exchange(exchange) = records::load(&app.store, &id).await? else {
                    let gone = Event::default()
                        .event("gone")
                        .json_data(json!({ "id": id }));
                    return Ok(Step {
                        events: vec![gone.expect("an id serializes")],
                        done: false,
                    });
                };
                let mut view = view(&app, &exchange, &viewer).await?;
                view["read"] = json!(records::read_ids(&app.store, &viewer).await?.contains(&id));
                let event = Event::default()
                    .event("exchange")
                    .json_data(view)
                    .expect("a view serializes");
                Ok(Step {
                    events: vec![event],
                    done: false,
                })
            }
        })
        .await?;
        return Ok(sse.into_response());
    }
    let read = records::read_ids(&app.store, &viewer).await?;
    let limit = filters.limit.unwrap_or(DEFAULT_LIMIT).min(MOST);
    let mut exchanges = Vec::new();
    for id in records::list(&app.store, &indexes, usize::MAX).await? {
        if exchanges.len() == limit {
            break;
        }
        let Found::Exchange(exchange) = records::load(&app.store, &id).await? else {
            continue;
        };
        let is_read = read.contains(&id);
        let wanted = wanted_state.is_none_or(|state| exchange.state == state)
            && filters.read.is_none_or(|wanted| wanted == is_read)
            && filters
                .question
                .is_none_or(|wanted| wanted == exchange.message.is_question());
        if wanted {
            let mut view = view(&app, &exchange, &viewer).await?;
            if is_recipient(&app, &exchange, &viewer).await? {
                view["read"] = json!(is_read);
            }
            exchanges.push(view);
        }
    }
    Ok(Json(json!({ "exchanges": exchanges })).into_response())
}
