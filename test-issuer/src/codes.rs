//! Authorization codes: the authorization endpoint signs a person in at
//! once and issues a code, and the token endpoint redeems it only for the
//! client, redirect URI, and PKCE verifier that asked for it, as a real
//! issuer would.

use std::collections::HashMap;
use std::sync::Arc;

use aws_lc_rs::digest::{SHA256, digest};
use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use axum::extract::{Query, State};
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Redirect;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::Deserialize;

use crate::{Client, Shared};

/// Who signed in, and the nonce their ID token carries.
pub(crate) struct SignIn {
    pub(crate) subject: String,
    pub(crate) name: String,
    pub(crate) nonce: String,
}

impl SignIn {
    /// A sign-in from a scripted code, `subject|name|nonce`.
    pub(crate) fn scripted(code: &str) -> Self {
        let mut parts = code.splitn(3, '|').map(str::to_owned);
        Self {
            subject: parts.next().unwrap_or_default(),
            name: parts.next().unwrap_or_default(),
            nonce: parts.next().unwrap_or_default(),
        }
    }
}

/// An authorization the token endpoint has yet to redeem.
pub(crate) struct Grant {
    client_id: String,
    redirect_uri: String,
    challenge: String,
    sign_in: SignIn,
}

/// The client id and secret of a token request: from HTTP Basic
/// authentication when it has one, or else the form's `client_id`.
fn credentials(headers: &HeaderMap, form: &HashMap<String, String>) -> (String, Option<String>) {
    let basic = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok()?.strip_prefix("Basic "))
        .and_then(|encoded| String::from_utf8(STANDARD.decode(encoded).ok()?).ok());
    match basic.as_deref().and_then(|pair| pair.split_once(':')) {
        Some((id, secret)) => (id.to_owned(), Some(secret.to_owned())),
        None => (form.get("client_id").cloned().unwrap_or_default(), None),
    }
}

impl Grant {
    pub(crate) fn redeem(
        self,
        client: &Client,
        headers: &HeaderMap,
        form: &HashMap<String, String>,
    ) -> Result<SignIn, StatusCode> {
        let (id, secret) = credentials(headers, form);
        let secret_matches = client.secret.is_none() || secret == client.secret;
        if id != self.client_id || !secret_matches {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let verifier = form.get("code_verifier").cloned().unwrap_or_default();
        let challenge = URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()));
        if form.get("redirect_uri") != Some(&self.redirect_uri) || challenge != self.challenge {
            return Err(StatusCode::BAD_REQUEST);
        }
        Ok(self.sign_in)
    }
}

#[derive(Deserialize)]
pub(crate) struct Authorization {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    state: String,
    nonce: String,
    code_challenge: String,
    code_challenge_method: String,
    /// Who to sign in, when it should not be the configured person.
    subject: Option<String>,
    name: Option<String>,
}

fn code() -> String {
    let mut bytes = [0u8; 32];
    SystemRandom::new().fill(&mut bytes).unwrap();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The authorization endpoint. It asks no one anything: it signs in the
/// person the request names, or the configured one, and sends the browser
/// straight back to the client with a code and the state.
pub(crate) async fn authorize(
    State(shared): State<Arc<Shared>>,
    Query(asked): Query<Authorization>,
) -> Result<Redirect, StatusCode> {
    let supported = asked.response_type == "code" && asked.code_challenge_method == "S256";
    if !supported || asked.client_id != shared.options.client.id {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut back = url::Url::parse(&asked.redirect_uri).map_err(|_| StatusCode::BAD_REQUEST)?;
    let code = code();
    back.query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &asked.state);
    let person = &shared.options.person;
    let grant = Grant {
        client_id: asked.client_id,
        redirect_uri: asked.redirect_uri,
        challenge: asked.code_challenge,
        sign_in: SignIn {
            subject: asked.subject.unwrap_or(person.subject.clone()),
            name: asked.name.unwrap_or(person.name.clone()),
            nonce: asked.nonce,
        },
    };
    shared.served.lock().unwrap().grants.insert(code, grant);
    Ok(Redirect::to(back.as_str()))
}
