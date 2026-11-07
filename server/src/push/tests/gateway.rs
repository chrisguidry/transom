//! The wake-up gateway, run as the process it is in production, so the
//! server is tested against the gateway's own answers. It signs in through
//! a stand-in for Cloud Run's metadata server and FCM, which answers by the
//! installation ID it is sent, as the gateway's own stand-in does:
//!
//! - `unregistered`: FCM's 404, which the gateway answers with 404.
//! - `busy`: FCM's 429 with `Retry-After: 30`, which the gateway answers
//!   with 503 and the same `Retry-After`.
//! - `foreign`: FCM's 403, as for a token of another Firebase project,
//!   which the gateway answers with 502.
//! - anything else: FCM's 200, which the gateway answers with 202.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};

/// Google's two services, standing in: each installation ID FCM was
/// sent, in order. The metadata server grants a token to anyone who asks:
/// the gateway's own tests check how it signs in.
struct Google {
    url: String,
    woken: Arc<Mutex<Vec<String>>>,
}

impl Google {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let woken = Arc::new(Mutex::new(Vec::new()));
        let recorded = woken.clone();
        let router = Router::new()
            .route(
                "/computeMetadata/v1/project/project-id",
                get(|| async { "transom-tests" }),
            )
            .route(
                "/computeMetadata/v1/instance/service-accounts/default/token",
                get(grant),
            )
            .route(
                "/v1/projects/{project}/messages:send",
                post(move |Json(body): Json<Value>| async move {
                    let token = body["message"]["fid"].as_str().unwrap().to_owned();
                    recorded.lock().unwrap().push(token.clone());
                    answer(&token)
                }),
            );
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self { url, woken }
    }
}

async fn grant() -> Json<Value> {
    Json(json!({ "access_token": "access", "expires_in": 3600, "token_type": "Bearer" }))
}

/// FCM's answer to a message for `token`.
fn answer(token: &str) -> Response {
    let error = |status: StatusCode, name: &str| {
        let body = json!({ "error": { "code": status.as_u16(), "status": name } });
        (status, Json(body)).into_response()
    };
    match token {
        "unregistered" => error(StatusCode::NOT_FOUND, "NOT_FOUND"),
        "busy" => (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "30")],
            Json(json!({ "error": { "code": 429, "status": "QUOTA_EXCEEDED" } })),
        )
            .into_response(),
        "foreign" => error(StatusCode::FORBIDDEN, "PERMISSION_DENIED"),
        _ => Json(json!({ "name": "projects/transom-tests/messages/1" })).into_response(),
    }
}

/// The gateway's binary. Cargo builds it beside the directory of this
/// test binary whenever it builds the workspace's tests, since the
/// gateway's own tests run it.
fn binary() -> PathBuf {
    let tests = std::env::current_exe().unwrap();
    tests
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("transom-gateway")
}

/// A running gateway that signs in through the stand-in, stopped when it
/// is dropped.
pub(super) struct Gateway {
    child: Child,
    /// Where the server asks it to wake a phone.
    pub(super) url: String,
    google: Google,
}

impl Gateway {
    pub(super) async fn start() -> Self {
        let google = Google::start().await;
        let mut child = Command::new(binary())
            .env("NO_COLOR", "1")
            .env("TRANSOM_GATEWAY_LISTEN", "127.0.0.1:0")
            .env("TRANSOM_GATEWAY_METADATA_URL", &google.url)
            .env("TRANSOM_GATEWAY_FCM_URL", &google.url)
            .stderr(Stdio::piped())
            .spawn()
            .expect("the gateway's binary is built: cargo build -p transom-gateway");
        let stderr = child.stderr.take().unwrap();
        // The gateway asks the stand-in for its project before it listens,
        // and the stand-in answers on this test's runtime, so the log is
        // read on a thread of its own while the runtime keeps serving.
        let address = tokio::task::spawn_blocking(move || {
            let mut lines = BufReader::new(stderr).lines();
            let address = lines
                .by_ref()
                .map(Result::unwrap)
                .find_map(|line| {
                    line.split_once("address=")
                        .map(|(_, rest)| rest.split_whitespace().next().unwrap().to_owned())
                })
                .unwrap();
            // The gateway logs each request, and one whose log line meets a
            // closed pipe is never answered, so the rest of the log is read
            // and dropped.
            std::thread::spawn(move || lines.for_each(drop));
            address
        })
        .await
        .unwrap();
        Self {
            child,
            url: format!("http://{address}/wake"),
            google,
        }
    }

    /// Each installation ID the gateway sent FCM a wake-up for.
    pub(super) fn woken(&self) -> Vec<String> {
        self.google.woken.lock().unwrap().clone()
    }

    /// Somewhere that answers as no gateway does: Google's stand-in, at a
    /// path it has no route for.
    pub(super) fn elsewhere(&self) -> String {
        format!("{}/wake", self.google.url)
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
