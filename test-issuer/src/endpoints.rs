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

/// An OAuth error response (RFC 6749, section 5.2).
pub(crate) type Refusal = (StatusCode, Json<Value>);

pub(crate) fn refusal(status: StatusCode, error: &str) -> Refusal {
    (status, Json(json!({ "error": error })))
}

/// The token endpoint. It redeems a code the authorization endpoint
/// issued, or a scripted code when the issuer takes them, for an ID token
/// and a refresh token. A refresh is answered as the test set.
pub(crate) async fn token(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> Result<Json<Value>, Refusal> {
    if form.get("grant_type").map(String::as_str) == Some("refresh_token") {
        return match refresh(&shared, form.get("refresh_token")) {
            Answer::Now(answer) => answer,
            Answer::After(wait, answer) => {
                tokio::time::sleep(wait).await;
                answer
            }
            Answer::Never => std::future::pending().await,
        };
    }
    let mut served = shared.served.lock().unwrap();
    let code = form.get("code").cloned().unwrap_or_default();
    let sign_in = if let Some(grant) = served.grants.remove(&code) {
        grant
            .redeem(&shared.options.client, &headers, &form)
            .map_err(|(status, error)| refusal(status, error))?
    } else if !shared.options.scripted_codes || code == "refused" {
        return Err(refusal(StatusCode::BAD_REQUEST, "invalid_grant"));
    } else if code == "wrong-client" {
        return Err(refusal(StatusCode::UNAUTHORIZED, "invalid_client"));
    } else if code == "fails" {
        return Err(refusal(StatusCode::INTERNAL_SERVER_ERROR, "server_error"));
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
    let id_token = if code.starts_with("stranger|") {
        Key::new("stranger", 99).sign(&claims)
    } else {
        signed(&served, &claims)
    };
    let refresh_token = served.refresh_token();
    Ok(Json(
        json!({ "access_token": "a", "id_token": id_token, "refresh_token": refresh_token }),
    ))
}

/// When the token endpoint answers a refresh.
enum Answer {
    Now(Result<Json<Value>, Refusal>),
    After(std::time::Duration, Result<Json<Value>, Refusal>),
    Never,
}

/// The answer to a refresh of `token`, as the test set.
fn refresh(shared: &Shared, token: Option<&String>) -> Answer {
    let mut served = shared.served.lock().unwrap();
    served.refreshes += 1;
    let refreshing = served.refreshing;
    let invalid_grant = || Err(refusal(StatusCode::BAD_REQUEST, "invalid_grant"));
    let mut rotate = |token: Option<&String>| {
        if token.is_some_and(|token| served.refresh_tokens.remove(token)) {
            let refresh_token = served.refresh_token();
            Ok(Json(
                json!({ "access_token": "a", "refresh_token": refresh_token }),
            ))
        } else {
            invalid_grant()
        }
    };
    Answer::Now(match refreshing {
        Refreshing::Accepts => rotate(token),
        Refreshing::AcceptsAfter(wait) => return Answer::After(wait, rotate(token)),
        Refreshing::Refuses => invalid_grant(),
        Refreshing::RefusesAfter(wait) => return Answer::After(wait, invalid_grant()),
        Refreshing::Errs(status, "") => {
            Err((StatusCode::from_u16(status).unwrap(), Json(Value::Null)))
        }
        Refreshing::Errs(status, error) => {
            Err(refusal(StatusCode::from_u16(status).unwrap(), error))
        }
        Refreshing::Fails => Err(refusal(StatusCode::INTERNAL_SERVER_ERROR, "server_error")),
        Refreshing::Hangs => return Answer::Never,
    })
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
