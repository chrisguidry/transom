//! A real token issuer for transom's tests: an HTTP server that serves a
//! discovery document and a key set, signs tokens with the matching
//! private keys, and signs a person in at once when a browser asks.
//!
//! transom's own tests start one in-process with [`TestIssuer::start`],
//! and steer it: rotating its keys, failing, refusing refreshes. The
//! `test-issuer` binary runs the same issuer as a process, for tests that
//! run against a real transom.

// A test issuer that cannot do what a test asked of it panics, which
// fails that test; returning errors would only move the unwrap into it.
#![allow(clippy::missing_panics_doc)]

mod codes;
mod endpoints;
mod keys;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::routing::{get, post};
use jsonwebtoken::{Algorithm, Header};
use serde_json::{Value, json};

use codes::Grant;
use keys::Key;

/// The audience of the access tokens the issuer signs, unless told
/// otherwise.
pub const AUDIENCE: &str = "transom-tests";

/// The client a browser signs in through, unless told otherwise.
pub const BROWSER_CLIENT: &str = "transom-browser";

/// How the token endpoint answers a refresh.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Refreshing {
    #[default]
    Accepts,
    Refuses,
    Fails,
}

/// The one client the issuer signs people in for.
#[derive(Clone)]
pub struct Client {
    pub id: String,
    /// The secret the client authenticates with, or `None` to take the
    /// client's word for its id.
    pub secret: Option<String>,
}

/// Who the authorization endpoint signs in when the request does not say.
#[derive(Clone)]
pub struct Person {
    pub subject: String,
    pub name: String,
}

pub struct Options {
    pub audience: String,
    pub client: Client,
    pub person: Person,
    /// Whether the token endpoint also takes codes that no authorization
    /// issued: `subject|name|nonce`, `refused`, and `no-id-token`. transom's
    /// tests use them to drive its callback without visiting the
    /// authorization endpoint first.
    pub scripted_codes: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            audience: AUDIENCE.to_owned(),
            client: Client {
                id: BROWSER_CLIENT.to_owned(),
                secret: None,
            },
            person: Person {
                subject: "tester".to_owned(),
                name: "tester".to_owned(),
            },
            scripted_codes: false,
        }
    }
}

#[derive(Default)]
struct Served {
    keys: Vec<Key>,
    refreshing: Refreshing,
    refreshes: usize,
    no_endpoints: bool,
    /// The issuer named in the discovery document, when it should differ
    /// from the issuer's own URL.
    claimed_issuer: Option<String>,
    failing: bool,
    key_fetches: usize,
    authorization: Option<String>,
    /// The codes the authorization endpoint issued and no one has redeemed.
    grants: HashMap<String, Grant>,
}

/// What every endpoint reads: the issuer's settings, and what it serves.
struct Shared {
    url: String,
    options: Options,
    served: Mutex<Served>,
}

impl Shared {
    /// `overrides` over the defaults for issuer, audience, and expiry.
    fn claims(&self, overrides: &Value) -> Value {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut claims = json!({
            "iss": self.url,
            "aud": self.options.audience,
            "iat": now,
            "exp": now + 300,
        });
        for (name, value) in overrides.as_object().unwrap() {
            claims[name] = value.clone();
        }
        claims
    }
}

/// `claims` signed with the newest key.
fn signed(served: &Served, claims: &Value) -> String {
    served.keys.last().unwrap().sign(claims)
}

pub struct TestIssuer {
    /// The issuer's URL, which its tokens carry in `iss`.
    pub url: String,
    shared: Arc<Shared>,
}

impl TestIssuer {
    /// An issuer at `url` with one signing key, which serves nothing until
    /// its [`router`](Self::router) is served.
    #[must_use]
    pub fn new(url: String, options: Options) -> Self {
        let served = Mutex::new(Served {
            keys: vec![Key::new("first", 1)],
            ..Served::default()
        });
        let shared = Arc::new(Shared {
            url: url.clone(),
            options,
            served,
        });
        Self { url, shared }
    }

    /// An issuer serving on a free port of this machine, which takes
    /// scripted codes.
    pub async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let options = Options {
            scripted_codes: true,
            ..Options::default()
        };
        let issuer = Self::new(url, options);
        let router = issuer.router();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        issuer
    }

    /// The issuer's endpoints.
    pub fn router(&self) -> Router {
        Router::new()
            .route(
                "/.well-known/openid-configuration",
                get(endpoints::discovery),
            )
            .route("/keys", get(endpoints::keys))
            .route("/authorize", get(codes::authorize))
            .route("/token", post(endpoints::token))
            .route("/access-token", get(endpoints::access_token))
            .with_state(self.shared.clone())
    }

    /// A token for `subject`, signed with the newest key.
    #[must_use]
    pub fn token(&self, subject: &str) -> String {
        self.token_with(&json!({ "sub": subject }))
    }

    /// A valid token for `subject` that also carries `preferred_username`.
    #[must_use]
    pub fn token_named(&self, subject: &str, name: &str) -> String {
        self.token_with(&json!({ "sub": subject, "preferred_username": name }))
    }

    /// A token with `claims` over the defaults for issuer, audience, and
    /// expiry, signed with the newest key.
    #[must_use]
    pub fn token_with(&self, claims: &Value) -> String {
        signed(&self.served(), &self.shared.claims(claims))
    }

    /// A valid token for `subject` whose header names no key.
    #[must_use]
    pub fn token_without_kid(&self, subject: &str) -> String {
        let claims = self.shared.claims(&json!({ "sub": subject }));
        let served = self.served();
        let header = Header::new(Algorithm::EdDSA);
        served.keys.first().unwrap().sign_with(&header, &claims)
    }

    /// A token signed with a key the issuer does not serve.
    #[must_use]
    pub fn token_signed_by_stranger(&self, subject: &str) -> String {
        let claims = self.shared.claims(&json!({ "sub": subject }));
        Key::new("stranger", 99).sign(&claims)
    }

    pub fn refresh(&self, refreshing: Refreshing) {
        self.served().refreshing = refreshing;
    }

    #[must_use]
    pub fn refreshes(&self) -> usize {
        self.served().refreshes
    }

    /// Leaves the authorization and token endpoints out of discovery, as a
    /// Kubernetes cluster does.
    pub fn publish_no_endpoints(&self) {
        self.served().no_endpoints = true;
    }

    /// Adds a new signing key, as an issuer does when it rotates.
    pub fn rotate(&self, kid: &str, seed: u8) {
        self.served().keys.push(Key::new(kid, seed));
    }

    /// Makes discovery fail from now on.
    pub fn fail(&self) {
        self.served().failing = true;
    }

    /// Makes discovery name `issuer` rather than the issuer's own URL.
    pub fn claim_issuer(&self, issuer: &str) {
        self.served().claimed_issuer = Some(issuer.to_owned());
    }

    #[must_use]
    pub fn key_fetches(&self) -> usize {
        self.served().key_fetches
    }

    /// The `Authorization` header of the latest discovery request.
    #[must_use]
    pub fn authorization(&self) -> Option<String> {
        self.served().authorization.clone()
    }

    fn served(&self) -> std::sync::MutexGuard<'_, Served> {
        self.shared.served.lock().unwrap()
    }
}
