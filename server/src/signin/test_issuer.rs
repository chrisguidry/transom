//! A real token issuer for tests: an HTTP server on a free port that
//! serves a discovery document and a key set, and signs tokens with the
//! matching private keys.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::SigningKey;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};

use crate::settings::IssuerSettings;

pub(crate) const AUDIENCE: &str = "transom-tests";

/// The DER prefix that wraps a 32-byte Ed25519 seed as a PKCS #8 key.
const PKCS8_ED25519_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

struct Key {
    kid: String,
    signing: SigningKey,
}

impl Key {
    fn new(kid: &str, seed: u8) -> Self {
        Self {
            kid: kid.to_owned(),
            signing: SigningKey::from_bytes(&[seed; 32]),
        }
    }

    fn jwk(&self) -> Value {
        json!({
            "kty": "OKP",
            "crv": "Ed25519",
            "alg": "EdDSA",
            "use": "sig",
            "kid": self.kid,
            "x": URL_SAFE_NO_PAD.encode(self.signing.verifying_key().as_bytes()),
        })
    }

    fn encoding_key(&self) -> EncodingKey {
        let mut der = PKCS8_ED25519_PREFIX.to_vec();
        der.extend_from_slice(self.signing.as_bytes());
        EncodingKey::from_ed_der(&der)
    }
}

#[derive(Default)]
struct Served {
    keys: Vec<Key>,
    /// The issuer named in the discovery document, when it should differ
    /// from the issuer's own URL.
    claimed_issuer: Option<String>,
    failing: bool,
    key_fetches: usize,
    authorization: Option<String>,
}

pub(crate) struct TestIssuer {
    pub(crate) url: String,
    served: Arc<Mutex<Served>>,
}

impl TestIssuer {
    pub(crate) async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let served = Arc::new(Mutex::new(Served {
            keys: vec![Key::new("first", 1)],
            ..Served::default()
        }));
        let router = Router::new()
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/keys", get(keys))
            .with_state((url.clone(), served.clone()));
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self { url, served }
    }

    pub(crate) fn settings(&self) -> IssuerSettings {
        IssuerSettings {
            url: self.url.clone(),
            audience: AUDIENCE.to_owned(),
            name_claim: Some("preferred_username".to_owned()),
            ca_file: None,
            token_file: None,
        }
    }

    /// A token for `subject`, signed with the newest key.
    pub(crate) fn token(&self, subject: &str) -> String {
        self.token_with(&json!({ "sub": subject }))
    }

    /// A valid token for `subject` that also carries `preferred_username`.
    pub(crate) fn token_named(&self, subject: &str, name: &str) -> String {
        self.token_with(&json!({ "sub": subject, "preferred_username": name }))
    }

    /// A token with `claims` over the defaults for issuer, audience, and
    /// expiry, signed with the newest key.
    pub(crate) fn token_with(&self, claims: &Value) -> String {
        let served = self.served.lock().unwrap();
        let key = served.keys.last().unwrap();
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(key.kid.clone());
        jsonwebtoken::encode(&header, &self.claims(claims), &key.encoding_key()).unwrap()
    }

    /// A valid token for `subject` whose header names no key.
    pub(crate) fn token_without_kid(&self, subject: &str) -> String {
        let served = self.served.lock().unwrap();
        let key = served.keys.first().unwrap();
        let claims = self.claims(&json!({ "sub": subject }));
        jsonwebtoken::encode(&Header::new(Algorithm::EdDSA), &claims, &key.encoding_key()).unwrap()
    }

    /// A token signed with a key the issuer does not serve.
    pub(crate) fn token_signed_by_stranger(&self, subject: &str) -> String {
        let stranger = Key::new("stranger", 99);
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(stranger.kid.clone());
        let claims = self.claims(&json!({ "sub": subject }));
        jsonwebtoken::encode(&header, &claims, &stranger.encoding_key()).unwrap()
    }

    fn claims(&self, overrides: &Value) -> Value {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut claims = json!({
            "iss": self.url,
            "aud": AUDIENCE,
            "iat": now,
            "exp": now + 300,
        });
        for (name, value) in overrides.as_object().unwrap() {
            claims[name] = value.clone();
        }
        claims
    }

    /// Adds a new signing key, as an issuer does when it rotates.
    pub(crate) fn rotate(&self, kid: &str, seed: u8) {
        self.served.lock().unwrap().keys.push(Key::new(kid, seed));
    }

    pub(crate) fn fail(&self) {
        self.served.lock().unwrap().failing = true;
    }

    pub(crate) fn claim_issuer(&self, issuer: &str) {
        self.served.lock().unwrap().claimed_issuer = Some(issuer.to_owned());
    }

    pub(crate) fn key_fetches(&self) -> usize {
        self.served.lock().unwrap().key_fetches
    }

    pub(crate) fn authorization(&self) -> Option<String> {
        self.served.lock().unwrap().authorization.clone()
    }
}

type Shared = (String, Arc<Mutex<Served>>);

async fn discovery(
    State((url, served)): State<Shared>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    let mut served = served.lock().unwrap();
    served.authorization = headers
        .get("authorization")
        .map(|value| value.to_str().unwrap().to_owned());
    if served.failing {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let issuer = served.claimed_issuer.clone().unwrap_or(url.clone());
    Ok(Json(
        json!({ "issuer": issuer, "jwks_uri": format!("{url}/keys") }),
    ))
}

async fn keys(State((_, served)): State<Shared>) -> Json<Value> {
    let mut served = served.lock().unwrap();
    served.key_fetches += 1;
    let keys: Vec<Value> = served.keys.iter().map(Key::jwk).collect();
    Json(json!({ "keys": keys }))
}
