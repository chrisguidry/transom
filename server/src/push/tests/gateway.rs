//! The wake-up gateway, run as the process it is in production, so the
//! server is tested against the gateway's own answers. It signs in to a
//! stand-in for Google's token endpoint and FCM, which answers by the
//! registration token it is sent, as the gateway's own stand-in does:
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
use std::sync::{Arc, LazyLock, Mutex};

use aws_lc_rs::encoding::{AsDer, Pkcs8V1Der};
use aws_lc_rs::rsa::{KeyPair, KeySize};
use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

/// The service account's private key, as PEM. One key serves every test,
/// since making one takes a while, and the stand-in grants any assertion:
/// the gateway's own tests check how it signs in.
static KEY: LazyLock<String> = LazyLock::new(|| {
    let key = KeyPair::generate(KeySize::Rsa2048).unwrap();
    let der = AsDer::<Pkcs8V1Der>::as_der(&key).unwrap();
    format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
        STANDARD.encode(der.as_ref())
    )
});

/// Google's two endpoints, standing in: each registration token FCM was
/// sent, in order.
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
        let router = Router::new().route("/token", post(grant)).route(
            "/v1/projects/{project}/messages:send",
            post(move |Json(body): Json<Value>| async move {
                let token = body["message"]["token"].as_str().unwrap().to_owned();
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

/// A running gateway with credentials for the stand-in, stopped when it
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
        let key = json!({
            "type": "service_account",
            "project_id": "transom-tests",
            "private_key_id": "test-key",
            "private_key": KEY.as_str(),
            "client_email": "gateway@transom-tests.iam.gserviceaccount.com",
            "token_uri": format!("{}/token", google.url),
        });
        let path = std::env::temp_dir().join(format!(
            "transom-gateway-key-{}.json",
            ulid::Ulid::generate()
        ));
        std::fs::write(&path, key.to_string()).unwrap();
        let mut child = Command::new(binary())
            .env("NO_COLOR", "1")
            .env("TRANSOM_GATEWAY_LISTEN", "127.0.0.1:0")
            .env("TRANSOM_GATEWAY_CREDENTIALS", &path)
            .env("TRANSOM_GATEWAY_FCM_URL", &google.url)
            .stderr(Stdio::piped())
            .spawn()
            .expect("the gateway's binary is built: cargo build -p transom-gateway");
        let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
        let address = lines
            .by_ref()
            .map(Result::unwrap)
            .find_map(|line| {
                line.split_once("address=")
                    .map(|(_, rest)| rest.split_whitespace().next().unwrap().to_owned())
            })
            .unwrap();
        // The gateway read its key before it listened.
        std::fs::remove_file(path).unwrap();
        // The gateway logs each request, and one whose log line meets a
        // closed pipe is never answered, so the rest of the log is read
        // and dropped.
        std::thread::spawn(move || lines.for_each(drop));
        Self {
            child,
            url: format!("http://{address}/wake"),
            google,
        }
    }

    /// Each registration token the gateway sent FCM a wake-up for.
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
