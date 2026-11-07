//! A stand-in for Google's two services the gateway calls, since tests
//! cannot reach the real ones: the Cloud Run metadata server, which names
//! the project and grants access tokens, and FCM's `messages:send`, which
//! takes only a token the metadata server granted and records each message
//! it is sent. The metadata server refuses any request without
//! `Metadata-Flavor: Google`, as Google's does.
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

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::Router;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};

pub(crate) const PROJECT: &str = "transom-tests";

/// Where the metadata server names the project.
pub(crate) const PROJECT_PATH: &str = "/computeMetadata/v1/project/project-id";

/// Where the metadata server grants an access token.
const TOKEN_PATH: &str = "/computeMetadata/v1/instance/service-accounts/default/token";

#[derive(Default)]
struct Recorded {
    /// How many access tokens the metadata server has granted.
    grants: u32,
    /// The grants below this number FCM no longer takes.
    revoked_below: u32,
    /// Whether the metadata server refuses to grant access tokens.
    refusing: bool,
    /// The project and the body of each message FCM was sent.
    messages: Vec<(String, Value)>,
}

struct Shared {
    expires_in: u64,
    recorded: Mutex<Recorded>,
}

pub(crate) struct StandIn {
    url: String,
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
            expires_in,
            recorded: Mutex::default(),
        });
        let metadata = Router::new()
            .route(PROJECT_PATH, get(|| async { PROJECT }))
            .route(TOKEN_PATH, get(token))
            .layer(axum::middleware::from_fn(flavored));
        let router = Router::new()
            .merge(metadata)
            .route("/v1/projects/{project}/messages:send", post(send))
            // A metadata server that answers, and grants nothing.
            .route(
                &format!("/nothing{TOKEN_PATH}"),
                get(|| async { Json(json!({})) }),
            )
            .with_state(shared.clone());
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self { url, shared }
    }

    /// Where the metadata server and FCM's API are.
    pub(crate) fn url(&self) -> &str {
        &self.url
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

    /// Makes the metadata server refuse to grant access tokens, as it does
    /// when the service has no service account.
    pub(crate) fn refuse_tokens(&self) {
        self.shared.recorded.lock().unwrap().refusing = true;
    }
}

/// Google's metadata server answers only a request that carries
/// `Metadata-Flavor: Google`, so the gateway's tests pass only when it
/// sends the header.
async fn flavored(request: Request, next: Next) -> Response {
    let flavor = request.headers().get("metadata-flavor");
    if flavor.is_none_or(|flavor| flavor != "Google") {
        return (
            StatusCode::FORBIDDEN,
            "Missing Metadata-Flavor:Google header",
        )
            .into_response();
    }
    next.run(request).await
}

async fn token(State(shared): State<Arc<Shared>>) -> Response {
    let mut recorded = shared.recorded.lock().unwrap();
    if recorded.refusing {
        return (StatusCode::NOT_FOUND, "no service account").into_response();
    }
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
