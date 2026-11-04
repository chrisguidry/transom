//! Individuals and groups, each at its own URL, `/@{name}`.
//!
//! - `PUT /@{name}` claims a free name. From a token with no individual yet,
//!   it creates the individual under that name, which is how an agent
//!   signs in. From an individual, it creates a group with that individual
//!   as its first member.
//! - `GET /@{name}` describes an identity. Any request from a token with no
//!   individual yet creates one under the issuer's name claim, which is how
//!   a person first signs in.
//! - `PATCH /@{name}` renames an identity. Its old name answers with a
//!   `308 Permanent Redirect` to the new one until someone else claims it.

mod members;
pub(crate) mod names;
pub(crate) mod records;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header::LOCATION;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;
use serde_json::json;

use crate::app::{App, Failure};
use crate::signin::Verified;
use names::Name;
use records::{Claim, Identity, Kind, Lookup};

pub(crate) fn router() -> Router<Arc<App>> {
    Router::new()
        .route(
            "/{target}",
            get(show)
                .put(claim)
                .patch(change)
                .post(crate::exchanges::send)
                // A send carries a message's images, up to 20 MiB, which
                // base64 in a JSON body makes a third larger.
                .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route("/{target}/exchanges", get(crate::exchanges::list))
        .route("/{target}/members", get(members::list))
        .route(
            "/{target}/members/{member}",
            axum::routing::put(members::invite).delete(members::remove),
        )
}

/// What a `/@{name}` path names.
pub(crate) enum Resolved {
    Current(Identity),
    /// A former name, answered with a redirect to its identity's new one.
    Moved(Response),
}

/// The identity a `/@{name}` path names, a redirect for a former name,
/// or `404` for a name no one holds.
pub(crate) async fn resolve(app: &App, path: &str) -> Result<Resolved, Failure> {
    let name = target(path)?;
    match records::lookup(&app.store, &name).await? {
        Lookup::Current(identity) => Ok(Resolved::Current(identity)),
        Lookup::Former(identity) => Ok(Resolved::Moved(redirect(&identity))),
        Lookup::Free => Err(Failure::new(
            StatusCode::NOT_FOUND,
            format!("no one is named {name}"),
        )),
    }
}

type Outcome = Result<Response, Failure>;

/// The name in a `/@{name}` path. Any other path at the root is not found.
fn target(path: &str) -> Result<Name, Failure> {
    let name = path
        .strip_prefix('@')
        .ok_or_else(|| Failure::new(StatusCode::NOT_FOUND, "not found"))?;
    Name::parse(name).map_err(|reason| Failure::new(StatusCode::BAD_REQUEST, reason))
}

/// An identity as JSON, with its webhook for those who manage it.
fn view(status: StatusCode, identity: &Identity, webhook: Option<&str>) -> Response {
    let mut body = json!({ "name": identity.name, "kind": identity.kind.as_str() });
    if let Some(webhook) = webhook {
        body["webhook"] = json!(webhook);
    }
    let body = Json(body);
    let location = format!("/@{}", identity.name);
    (status, [(LOCATION, location)], body).into_response()
}

fn redirect(identity: &Identity) -> Response {
    let location = format!("/@{}", identity.name);
    (StatusCode::PERMANENT_REDIRECT, [(LOCATION, location)]).into_response()
}

fn taken(name: &Name) -> Failure {
    Failure::new(StatusCode::FORBIDDEN, format!("the name {name} is taken"))
}

/// The individual the caller's token is bound to. A token with no
/// individual yet gets one, named by the issuer's name claim.
pub(crate) async fn caller(app: &App, verified: &Verified) -> Result<Identity, Failure> {
    let store = &app.store;
    if let Some(id) = records::bound(store, &verified.issuer, &verified.subject).await? {
        return Ok(records::identity(store, &id).await?);
    }
    let name = verified
        .name
        .as_deref()
        .and_then(|name| Name::parse(&name.to_lowercase()).ok())
        .ok_or_else(|| {
            Failure::new(
                StatusCode::CONFLICT,
                "your token carries no usable name; choose one with a PUT to /@{name}",
            )
        })?;
    // A token with no individual cannot hold a former name, so anything
    // but a new individual means the name belongs to someone else.
    let Claim::Created(id) =
        records::claim(store, &name, &verified.issuer, &verified.subject).await?
    else {
        return Err(Failure::new(
            StatusCode::CONFLICT,
            format!("the name {name} is taken; choose another with a PUT to /@{{name}}"),
        ));
    };
    Ok(records::identity(store, &id).await?)
}

/// Whether `caller` may change `identity`: it is that individual, or a
/// member of that group.
async fn manages(app: &App, identity: &Identity, caller: Option<&str>) -> Result<bool, Failure> {
    let Some(caller) = caller else {
        return Ok(false);
    };
    if identity.id == caller {
        return Ok(true);
    }
    Ok(
        identity.kind == Kind::Group
            && records::is_member(&app.store, &identity.id, caller).await?,
    )
}

async fn show(
    State(app): State<Arc<App>>,
    Path(path): Path<String>,
    verified: Verified,
) -> Outcome {
    let name = target(&path)?;
    let caller = caller(&app, &verified).await?;
    match records::lookup(&app.store, &name).await? {
        Lookup::Current(identity) => {
            let webhook = if manages(&app, &identity, Some(&caller.id)).await? {
                records::webhook(&app.store, &identity.id).await?
            } else {
                None
            };
            Ok(view(StatusCode::OK, &identity, webhook.as_deref()))
        }
        Lookup::Former(identity) => Ok(redirect(&identity)),
        Lookup::Free => Err(Failure::new(
            StatusCode::NOT_FOUND,
            format!("no one is named {name}"),
        )),
    }
}

async fn claim(
    State(app): State<Arc<App>>,
    Path(path): Path<String>,
    verified: Verified,
) -> Outcome {
    let name = target(&path)?;
    let store = &app.store;
    match records::claim(store, &name, &verified.issuer, &verified.subject).await? {
        Claim::Current { id, caller } => {
            let identity = records::identity(store, &id).await?;
            if manages(&app, &identity, caller.as_deref()).await? {
                Ok(view(StatusCode::OK, &identity, None))
            } else {
                Err(taken(&name))
            }
        }
        Claim::Former(id) => Ok(redirect(&records::identity(store, &id).await?)),
        Claim::Created(id) => {
            let identity = records::identity(store, &id).await?;
            Ok(view(StatusCode::CREATED, &identity, None))
        }
    }
}

/// A change to an identity: a new name, a webhook, or both. An empty
/// `webhook` removes the identity's webhook.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    name: Option<String>,
    webhook: Option<String>,
}

/// A webhook must be an `http` or `https` URL, since transom POSTs to it.
fn webhook(url: &str) -> Result<Option<&str>, Failure> {
    if url.is_empty() {
        return Ok(None);
    }
    match url::Url::parse(url).map(|parsed| parsed.scheme().to_owned()) {
        Ok(scheme) if scheme == "https" || scheme == "http" => Ok(Some(url)),
        _ => Err(Failure::new(
            StatusCode::BAD_REQUEST,
            format!("{url:?} is not an http or https URL"),
        )),
    }
}

async fn change(
    State(app): State<Arc<App>>,
    Path(path): Path<String>,
    verified: Verified,
    Json(change): Json<Change>,
) -> Outcome {
    let name = target(&path)?;
    let new_name = change
        .name
        .as_deref()
        .map(|name| {
            Name::parse(name).map_err(|reason| Failure::new(StatusCode::BAD_REQUEST, reason))
        })
        .transpose()?;
    let new_webhook = change.webhook.as_deref().map(webhook).transpose()?;
    if new_name.is_none() && new_webhook.is_none() {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "change a name, a webhook, or both",
        ));
    }
    let caller = caller(&app, &verified).await?;
    let identity = match records::lookup(&app.store, &name).await? {
        Lookup::Current(identity) => identity,
        Lookup::Former(identity) => return Ok(redirect(&identity)),
        Lookup::Free => {
            return Err(Failure::new(
                StatusCode::NOT_FOUND,
                format!("no one is named {name}"),
            ));
        }
    };
    if !manages(&app, &identity, Some(&caller.id)).await? {
        return Err(Failure::new(
            StatusCode::FORBIDDEN,
            format!("you may not change {name}"),
        ));
    }
    if let Some(new_name) = &new_name
        && !records::rename(&app.store, &identity, new_name).await?
    {
        return Err(Failure::new(
            StatusCode::CONFLICT,
            format!("the name {new_name} is taken"),
        ));
    }
    if let Some(new_webhook) = new_webhook {
        records::set_webhook(&app.store, &identity.id, new_webhook).await?;
    }
    let identity = records::identity(&app.store, &identity.id).await?;
    let webhook = records::webhook(&app.store, &identity.id).await?;
    Ok(view(StatusCode::OK, &identity, webhook.as_deref()))
}
