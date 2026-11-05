//! Marking many exchanges read at once, at `POST /@{name}/read`: the
//! exchanges a list names, or every one the caller received up to a time.
//! Each read reaches the caller's feeds as a single read does, and each of
//! the caller's devices hears of them all at once.

use std::collections::HashSet;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use redis::AsyncCommands;
use serde::Deserialize;
use serde_json::json;

use super::records::{self, Found};
use super::{changes, is_recipient};
use crate::app::{App, Failure};
use crate::identities::caller;
use crate::identities::records::{self as identities, Identity};
use crate::signin::Verified;

/// Which exchanges to mark read, as a request's body or an MCP tool's
/// arguments name them: `exchanges` or `before`, and not both.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Marking {
    /// The ids of the exchanges to mark read.
    exchanges: Option<Vec<String>>,
    /// A time, such as 2026-11-05T12:00:00Z: every exchange you received
    /// up to then is marked read.
    before: Option<String>,
}

/// The exchanges a [`Marking`] names, once it is checked.
enum Which {
    Listed(Vec<String>),
    /// Received up to this time, in milliseconds since the epoch.
    Before(i64),
}

impl Marking {
    /// A marking from a form, where each id is one more `exchanges` field.
    fn from_form(body: &[u8]) -> Result<Self, String> {
        let mut marking = Self::default();
        for (name, value) in url::form_urlencoded::parse(body) {
            match name.as_ref() {
                "exchanges" => marking
                    .exchanges
                    .get_or_insert_default()
                    .push(value.into_owned()),
                "before" => marking.before = Some(value.into_owned()),
                name => return Err(format!("{name} is not a field of a marking")),
            }
        }
        Ok(marking)
    }

    fn which(self) -> Result<Which, String> {
        match (self.exchanges, self.before) {
            (Some(ids), None) => Ok(Which::Listed(ids)),
            (None, Some(before)) => before
                .parse::<jiff::Timestamp>()
                .map(|at| Which::Before(at.as_millisecond()))
                .map_err(|error| format!("before is not a time: {error}")),
            (Some(_), Some(_)) => Err("give exchanges or before, not both".to_owned()),
            (None, None) => Err("give the exchanges to mark read, or before".to_owned()),
        }
    }
}

/// Reads a marking from a JSON, YAML, or form body.
fn read(headers: &HeaderMap, body: &Bytes) -> Result<Marking, String> {
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match essence.as_str() {
        "application/json" => serde_json::from_slice(body)
            .map_err(|error| format!("the JSON body is not a marking: {error}")),
        "application/yaml" | "application/x-yaml" | "text/yaml" => {
            let text = std::str::from_utf8(body).map_err(|_| "the body is not UTF-8")?;
            serde_saphyr::from_str(text)
                .map_err(|error| format!("the YAML body is not a marking: {error}"))
        }
        "application/x-www-form-urlencoded" => Marking::from_form(body),
        _ => Err("the body must be JSON, YAML, or a form".to_owned()),
    }
}

pub(crate) async fn mark(
    State(app): State<Arc<App>>,
    Path(target): Path<String>,
    headers: HeaderMap,
    verified: Verified,
    body: Bytes,
) -> Result<Response, Failure> {
    let me = caller(&app, &verified).await?;
    let name = target
        .strip_prefix('@')
        .ok_or_else(|| Failure::new(StatusCode::NOT_FOUND, "not found"))?;
    if name != me.name {
        return Err(Failure::new(
            StatusCode::FORBIDDEN,
            "read state is private to each recipient",
        ));
    }
    let marking =
        read(&headers, &body).map_err(|reason| Failure::new(StatusCode::BAD_REQUEST, reason))?;
    let read = mark_read(&app, &me, marking).await?;
    Ok(Json(json!({ "read": read })).into_response())
}

/// Marks what `marking` names read for `reader`, and returns the ids it
/// changed. An exchange already read is left out, and so is one that
/// `reader` did not receive, as for a single read, which refuses it.
pub(crate) async fn mark_read(
    app: &App,
    reader: &Identity,
    marking: Marking,
) -> Result<Vec<String>, Failure> {
    let which = marking
        .which()
        .map_err(|reason| Failure::new(StatusCode::BAD_REQUEST, reason))?;
    let named = match which {
        Which::Listed(ids) => ids,
        Which::Before(before) => received_before(app, reader, before).await?,
    };
    let mut seen = HashSet::new();
    let mut exchanges = Vec::new();
    for id in named.into_iter().filter(|id| seen.insert(id.clone())) {
        if let Found::Exchange(exchange) = records::load(&app.store, &id).await?
            && is_recipient(app, &exchange, &reader.id).await?
        {
            exchanges.push(*exchange);
        }
    }
    let ids: Vec<&str> = exchanges
        .iter()
        .map(|exchange| exchange.id.as_str())
        .collect();
    let changed = changes::read_all(&app.store, &reader.id, &ids).await?;
    let changed_ids: HashSet<&str> = changed.iter().map(String::as_str).collect();
    let changed_exchanges: Vec<_> = exchanges
        .iter()
        .filter(|exchange| changed_ids.contains(exchange.id.as_str()))
        .collect();
    crate::push::read_many(app, &reader.id, &changed_exchanges).await?;
    Ok(changed)
}

/// The ids of the exchanges `reader` received, itself or through one of
/// its groups, up to `before`, oldest first.
async fn received_before(
    app: &App,
    reader: &Identity,
    before: i64,
) -> Result<Vec<String>, Failure> {
    let mut connection = app.store.connection().await?;
    let mut recipients = vec![reader.id.clone()];
    recipients.extend(identities::groups_of(&app.store, &reader.id).await?);
    let mut ids = Vec::new();
    for recipient in recipients {
        let received: Vec<String> = connection
            .zrangebyscore(
                app.store.key(&format!("received:{recipient}")),
                "-inf",
                before,
            )
            .await?;
        ids.extend(received);
    }
    // Exchange ids are ULIDs, which sort by when they were made.
    ids.sort();
    Ok(ids)
}
