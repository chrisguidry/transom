//! A group's members, at `/@{group}/members/@{name}`. Members are peers:
//! any member can list the others, invite someone, or remove someone, and
//! joining takes the invitee's consent. When the last member leaves, the
//! group is deleted with every exchange it received.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::records::{self, Identity, Kind};
use super::{Resolved, caller, resolve};
use crate::app::{App, Failure};
use crate::signin::Verified;

type Outcome = Result<Response, Failure>;

/// The group a path names, if the caller is one of its members.
async fn membership(
    app: &App,
    path: &str,
    verified: &Verified,
) -> Result<Result<(Identity, Identity), Response>, Failure> {
    let group = match resolve(app, path).await? {
        Resolved::Current(group) => group,
        Resolved::Moved(response) => return Ok(Err(response)),
    };
    if group.kind != Kind::Group {
        return Err(Failure::new(
            StatusCode::NOT_FOUND,
            format!("@{} is not a group", group.name),
        ));
    }
    let caller = caller(app, verified).await?;
    if !records::is_member(&app.store, &group.id, &caller.id).await? {
        return Err(Failure::new(
            StatusCode::FORBIDDEN,
            format!("only members of @{} may do that", group.name),
        ));
    }
    Ok(Ok((group, caller)))
}

pub(super) async fn list(
    State(app): State<Arc<App>>,
    Path(path): Path<String>,
    headers: axum::http::HeaderMap,
    verified: Verified,
) -> Outcome {
    let (group, me) = match membership(&app, &path, &verified).await? {
        Ok(found) => found,
        Err(moved) => return Ok(moved),
    };
    let mut names = Vec::new();
    for id in records::members(&app.store, &group.id).await? {
        names.push(format!(
            "@{}",
            records::identity(&app.store, &id).await?.name
        ));
    }
    names.sort();
    if crate::web::wants_html(&headers) {
        return crate::web::pages::members(&me.name, &group.name, names);
    }
    Ok(Json(json!({ "members": names })).into_response())
}

/// The individual a member path names.
async fn member(app: &App, path: &str) -> Result<Identity, Failure> {
    match resolve(app, path).await? {
        Resolved::Current(identity) if identity.kind == Kind::Individual => Ok(identity),
        Resolved::Current(identity) => Err(Failure::new(
            StatusCode::BAD_REQUEST,
            format!(
                "@{} is a group, and a group's members are individuals",
                identity.name
            ),
        )),
        Resolved::Moved(_) => Err(Failure::new(StatusCode::NOT_FOUND, "that name has moved")),
    }
}

pub(super) async fn invite(
    State(app): State<Arc<App>>,
    Path((path, member_path)): Path<(String, String)>,
    verified: Verified,
) -> Outcome {
    let (group, caller) = match membership(&app, &path, &verified).await? {
        Ok(found) => found,
        Err(moved) => return Ok(moved),
    };
    let invitee = member(&app, &member_path).await?;
    if records::is_member(&app.store, &group.id, &invitee.id).await? {
        return Ok(Json(json!({ "member": format!("@{}", invitee.name) })).into_response());
    }
    let invitation = crate::exchanges::invite(&app, &group, &invitee, &caller.name).await?;
    let body = json!({ "invited": format!("@{}", invitee.name), "exchange": format!("/exchanges/{invitation}") });
    Ok((StatusCode::ACCEPTED, Json(body)).into_response())
}

pub(super) async fn remove(
    State(app): State<Arc<App>>,
    Path((path, member_path)): Path<(String, String)>,
    verified: Verified,
) -> Outcome {
    let (group, _) = match membership(&app, &path, &verified).await? {
        Ok(found) => found,
        Err(moved) => return Ok(moved),
    };
    let leaving = member(&app, &member_path).await?;
    if !records::is_member(&app.store, &group.id, &leaving.id).await? {
        return Err(Failure::new(
            StatusCode::NOT_FOUND,
            format!("@{} is not a member", leaving.name),
        ));
    }
    if records::remove_member(&app.store, &group.id, &leaving.id).await? == 0 {
        crate::exchanges::bury(&app, &group.id).await?;
        records::delete_group(&app.store, &group).await?;
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}
