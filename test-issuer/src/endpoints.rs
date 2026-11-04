//! Discovery, the key set, the token endpoint, and the access tokens a
//! script asks for.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::codes::SignIn;
use crate::keys::Key;
use crate::{Refreshing, Shared, signed};

pub(crate) async fn discovery(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    let url = &shared.url;
    let mut served = shared.served.lock().unwrap();
    served.authorization = headers
        .get("authorization")
        .map(|value| value.to_str().unwrap().to_owned());
    if served.failing {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let issuer = served.claimed_issuer.clone().unwrap_or(url.clone());
    if served.no_endpoints {
        return Ok(Json(
            json!({ "issuer": issuer, "jwks_uri": format!("{url}/keys") }),
        ));
    }
    Ok(Json(json!({
        "issuer": issuer,
        "jwks_uri": format!("{url}/keys"),
        "authorization_endpoint": format!("{url}/authorize"),
        "token_endpoint": format!("{url}/token"),
    })))
}

pub(crate) async fn keys(State(shared): State<Arc<Shared>>) -> Json<Value> {
    let mut served = shared.served.lock().unwrap();
    served.key_fetches += 1;
    let keys: Vec<Value> = served.keys.iter().map(Key::jwk).collect();
    Json(json!({ "keys": keys }))
}

/// The token endpoint. It redeems a code the authorization endpoint
/// issued, or a scripted code when the issuer takes them, for an ID token
/// and a refresh token. A refresh is answered as the test set.
pub(crate) async fn token(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let mut served = shared.served.lock().unwrap();
    if form.get("grant_type").map(String::as_str) == Some("refresh_token") {
        served.refreshes += 1;
        return match served.refreshing {
            Refreshing::Accepts => Ok(Json(
                json!({ "access_token": "a", "refresh_token": "refreshed" }),
            )),
            Refreshing::Refuses => Err(StatusCode::BAD_REQUEST),
            Refreshing::Fails => Err(StatusCode::INTERNAL_SERVER_ERROR),
        };
    }
    let code = form.get("code").cloned().unwrap_or_default();
    let sign_in = if let Some(grant) = served.grants.remove(&code) {
        grant.redeem(&shared.options.client, &headers, &form)?
    } else if !shared.options.scripted_codes || code == "refused" {
        return Err(StatusCode::BAD_REQUEST);
    } else if code == "no-id-token" {
        return Ok(Json(json!({ "access_token": "a" })));
    } else {
        SignIn::scripted(&code)
    };
    let claims = shared.claims(&json!({
        "aud": shared.options.client.id,
        "sub": sign_in.subject,
        "preferred_username": sign_in.name,
        "nonce": sign_in.nonce,
    }));
    let id_token = signed(&served, &claims);
    Ok(Json(
        json!({ "access_token": "a", "id_token": id_token, "refresh_token": "first" }),
    ))
}

#[derive(Deserialize)]
pub(crate) struct Subject {
    subject: Option<String>,
    name: Option<String>,
}

/// An access token for transom, for the subject and name in the query,
/// each defaulting to the configured person's, as plain text a script can
/// put in an `Authorization` header.
pub(crate) async fn access_token(
    State(shared): State<Arc<Shared>>,
    Query(asked): Query<Subject>,
) -> String {
    let person = &shared.options.person;
    let claims = shared.claims(&json!({
        "sub": asked.subject.unwrap_or(person.subject.clone()),
        "preferred_username": asked.name.unwrap_or(person.name.clone()),
    }));
    signed(&shared.served.lock().unwrap(), &claims)
}
