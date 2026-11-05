//! A stand-in for Google's two endpoints the gateway calls, since tests
//! cannot reach the real ones: the OAuth token endpoint, which checks the
//! signed JWT against the test key before it grants an access token, and
//! FCM's `messages:send`, which takes only a token it granted and records
//! each message it is sent.
//!
//! FCM answers by the installation ID in the message, so a test picks
//! FCM's answer with the token it wakes:
//!
//! - `unregistered`: 404, as for an app that was uninstalled.
//! - `invalid`: 400 `INVALID_ARGUMENT`, as for a token that was never one.
//! - `busy`: 429 with `Retry-After: 30`.
//! - `failing`: 500, with no body FCM's errors have.
//! - `foreign`: 403 `PERMISSION_DENIED`, as for a token of another project.
//! - anything else: 200.

use std::sync::{Arc, LazyLock, Mutex};

use aws_lc_rs::encoding::AsDer;
use aws_lc_rs::rsa::{KeyPair, KeySize};
use aws_lc_rs::signature::KeyPair as _;
use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde_json::{Value, json};

pub(crate) const PROJECT: &str = "transom-tests";
const EMAIL: &str = "gateway@transom-tests.iam.gserviceaccount.com";
const KEY_ID: &str = "test-key";

/// One RSA key for every test, since making one takes a while.
static KEY: LazyLock<KeyPair> = LazyLock::new(|| KeyPair::generate(KeySize::Rsa2048).unwrap());

/// A service account key file, as Google writes one, for the test key,
/// signing in at `token_uri`.
pub(crate) fn key_file(token_uri: &str) -> String {
    let der = AsDer::<aws_lc_rs::encoding::Pkcs8V1Der>::as_der(&*KEY).unwrap();
    let base64 = STANDARD.encode(der.as_ref());
    let lines: Vec<&str> = base64
        .as_bytes()
        .chunks(64)
        .map(|line| std::str::from_utf8(line).unwrap())
        .collect();
    let pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
        lines.join("\n")
    );
    json!({
        "type": "service_account",
        "project_id": PROJECT,
        "private_key_id": KEY_ID,
        "private_key": pem,
        "client_email": EMAIL,
        "client_id": "1",
        "token_uri": token_uri,
    })
    .to_string()
}

#[derive(Default)]
struct Recorded {
    /// How many access tokens the token endpoint has granted.
    grants: u32,
    /// The grants below this number FCM no longer takes.
    revoked_below: u32,
    /// The project and the body of each message FCM was sent.
    messages: Vec<(String, Value)>,
}

struct Shared {
    url: String,
    expires_in: u64,
    recorded: Mutex<Recorded>,
}

pub(crate) struct StandIn {
    shared: Arc<Shared>,
}

impl StandIn {
    /// A stand-in whose access tokens last an hour, as Google's do.
    pub(crate) async fn start() -> Self {
        Self::granting_for(3600).await
    }

    /// A stand-in whose access tokens last `expires_in` seconds.
    pub(crate) async fn granting_for(expires_in: u64) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let shared = Arc::new(Shared {
            url: format!("http://{}", listener.local_addr().unwrap()),
            expires_in,
            recorded: Mutex::default(),
        });
        let router = Router::new()
            .route("/token", post(token))
            .route("/v1/projects/{project}/messages:send", post(send))
            // A token endpoint that answers, and grants nothing.
            .route("/nothing", post(|| async { Json(json!({})) }))
            .with_state(shared.clone());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self { shared }
    }

    /// Where FCM's API is.
    pub(crate) fn url(&self) -> &str {
        &self.shared.url
    }

    /// Where the token endpoint is.
    pub(crate) fn token_uri(&self) -> String {
        format!("{}/token", self.shared.url)
    }

    pub(crate) fn grants(&self) -> u32 {
        self.shared.recorded.lock().unwrap().grants
    }

    /// The project and body of each message FCM was sent.
    pub(crate) fn messages(&self) -> Vec<(String, Value)> {
        self.shared.recorded.lock().unwrap().messages.clone()
    }

    /// Makes FCM refuse every access token granted so far.
    pub(crate) fn revoke(&self) {
        let mut recorded = self.shared.recorded.lock().unwrap();
        recorded.revoked_below = recorded.grants + 1;
    }
}

/// The JWT bearer grant: the assertion must be signed with the test key,
/// by the test account, for this endpoint, asking to send FCM messages.
async fn token(State(shared): State<Arc<Shared>>, body: String) -> Response {
    let fields: Vec<(&str, &str)> = body
        .split('&')
        .filter_map(|field| field.split_once('='))
        .collect();
    let grant_type = "urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer";
    let assertion = fields
        .iter()
        .find(|(name, _)| *name == "assertion")
        .map_or("", |(_, value)| *value);
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[format!("{}/token", shared.url)]);
    validation.set_issuer(&[EMAIL]);
    validation.set_required_spec_claims(&["exp", "iat", "aud", "iss"]);
    let public = DecodingKey::from_rsa_der(KEY.public_key().as_ref());
    let verified = jsonwebtoken::decode::<Value>(assertion, &public, &validation);
    let granted = fields.contains(&("grant_type", grant_type))
        && verified.as_ref().is_ok_and(|token| {
            token.header.kid.as_deref() == Some(KEY_ID)
                && token.claims["scope"] == "https://www.googleapis.com/auth/firebase.messaging"
        });
    if !granted {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_grant" })),
        )
            .into_response();
    }
    let mut recorded = shared.recorded.lock().unwrap();
    recorded.grants += 1;
    Json(json!({
        "access_token": format!("access-{}", recorded.grants),
        "expires_in": shared.expires_in,
        "token_type": "Bearer",
    }))
    .into_response()
}

/// FCM's error body, as its HTTP v1 API writes one.
fn error(status: StatusCode, name: &str) -> Response {
    let body = json!({ "error": { "code": status.as_u16(), "status": name } });
    (status, Json(body)).into_response()
}

async fn send(
    State(shared): State<Arc<Shared>>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let mut recorded = shared.recorded.lock().unwrap();
    let granted = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer access-"))
        .and_then(|grant| grant.parse::<u32>().ok())
        .is_some_and(|grant| grant >= recorded.revoked_below && grant <= recorded.grants);
    if !granted {
        return error(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
    }
    let token = body["message"]["fid"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    recorded.messages.push((project, body));
    match token.as_str() {
        "unregistered" => error(StatusCode::NOT_FOUND, "NOT_FOUND"),
        "invalid" => error(StatusCode::BAD_REQUEST, "INVALID_ARGUMENT"),
        "busy" => (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "30")],
            Json(json!({ "error": { "code": 429, "status": "QUOTA_EXCEEDED" } })),
        )
            .into_response(),
        "failing" => (StatusCode::INTERNAL_SERVER_ERROR, "oops").into_response(),
        "foreign" => error(StatusCode::FORBIDDEN, "PERMISSION_DENIED"),
        _ => Json(json!({ "name": format!("projects/{PROJECT}/messages/1") })).into_response(),
    }
}
