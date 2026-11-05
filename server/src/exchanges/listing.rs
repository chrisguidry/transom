//! An identity's exchanges, sent and received, at `/@{name}/exchanges`:
//! the inbox, and as an event stream, the live feed (`feed.rs`).

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use super::records::{self, Found, State as ExchangeState};
use super::waiting;
use super::{is_recipient, view};
use crate::app::{App, Failure};
use crate::identities::records::{self as identities, Identity, Kind};
use crate::identities::{Resolved, caller, resolve};
use crate::signin::Verified;

#[derive(Default, Deserialize)]
pub(crate) struct Filters {
    pub(crate) state: Option<String>,
    pub(crate) read: Option<bool>,
    pub(crate) question: Option<bool>,
    pub(crate) limit: Option<usize>,
}

/// The state `filters` asks for, if any.
fn wanted(filters: &Filters) -> Result<Option<ExchangeState>, Failure> {
    filters
        .state
        .as_deref()
        .map(|state| {
            ExchangeState::parse(state).ok_or_else(|| {
                Failure::new(StatusCode::BAD_REQUEST, format!("{state:?} is not a state"))
            })
        })
        .transpose()
}

/// The sorted sets of exchange ids an identity's list draws on: what it
/// sent and received, and for an individual, what its groups received.
async fn indexes(app: &App, identity: &Identity) -> Result<Vec<String>, Failure> {
    let mut indexes = vec![
        format!("sent:{}", identity.id),
        format!("received:{}", identity.id),
    ];
    if identity.kind == Kind::Individual {
        for group in identities::groups_of(&app.store, &identity.id).await? {
            indexes.push(format!("received:{group}"));
        }
    }
    Ok(indexes)
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
    let me = caller(&app, &verified).await?;
    let viewer = me.id.clone();
    let member = identity.kind == Kind::Group
        && identities::is_member(&app.store, &identity.id, &viewer).await?;
    if identity.id != viewer && !member {
        return Err(Failure::new(
            StatusCode::FORBIDDEN,
            "an identity's exchanges are its own",
        ));
    }
    let wanted_state = wanted(&filters)?;
    if waiting::wants_events(&headers) {
        return super::feed::follow(app, identity, viewer).await;
    }
    let indexes = indexes(&app, &identity).await?;
    let exchanges = listed(&app, &indexes, &viewer, wanted_state, &filters).await?;
    if crate::web::wants_html(&headers) {
        return crate::web::pages::inbox(&me.name, &identity.name, &exchanges);
    }
    Ok(Json(json!({ "exchanges": exchanges })).into_response())
}

/// The exchanges in `indexes` that pass `filters`, newest first, as
/// `viewer` sees them.
async fn listed(
    app: &App,
    indexes: &[String],
    viewer: &str,
    wanted_state: Option<ExchangeState>,
    filters: &Filters,
) -> Result<Vec<serde_json::Value>, Failure> {
    let read = records::read_ids(&app.store, viewer).await?;
    // Retention bounds what the server keeps, so the list is every exchange
    // it keeps unless the caller asks for fewer.
    let limit = filters.limit.unwrap_or(usize::MAX);
    let mut exchanges = Vec::new();
    for id in records::list(&app.store, indexes, usize::MAX).await? {
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
            let mut view = view(app, &exchange, viewer).await?;
            if is_recipient(app, &exchange, viewer).await? {
                view["read"] = json!(is_read);
            }
            exchanges.push(view);
        }
    }
    Ok(exchanges)
}

/// `identity`'s exchanges that pass `filters`, as `identity` sees them.
pub(crate) async fn inbox(
    app: &App,
    identity: &Identity,
    filters: &Filters,
) -> Result<Vec<serde_json::Value>, Failure> {
    let wanted_state = wanted(filters)?;
    let indexes = indexes(app, identity).await?;
    listed(app, &indexes, &identity.id, wanted_state, filters).await
}
