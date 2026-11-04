//! Push notifications end to end: a push service in the test captures each
//! POST, and a browser's key decrypts it.

mod delivery;
mod devices;
mod encryption;
mod reading;

use std::sync::{Arc, Mutex};

use aws_lc_rs::agreement::{ECDH_P256, PrivateKey};
use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};

use super::encryption::decrypt;
use crate::app::testing::{TestApp, decide, request};

const JSON: (&str, &str) = ("content-type", "application/json");

/// Each POST a push service received: its headers and its body.
type Received = Arc<Mutex<Vec<(HeaderMap, Vec<u8>)>>>;

/// A push service that records each POST it receives and answers with
/// `status`.
struct PushService {
    url: String,
    received: Received,
    status: Arc<Mutex<StatusCode>>,
}

impl PushService {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/push/abc", listener.local_addr().unwrap());
        let received = Arc::new(Mutex::new(Vec::new()));
        let status = Arc::new(Mutex::new(StatusCode::CREATED));
        let (got, answer) = (received.clone(), status.clone());
        let router = Router::new().route(
            "/push/abc",
            post(
                move |headers: HeaderMap, body: axum::body::Bytes| async move {
                    got.lock().unwrap().push((headers, body.to_vec()));
                    *answer.lock().unwrap()
                },
            ),
        );
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self {
            url,
            received,
            status,
        }
    }

    fn answer(&self, status: StatusCode) {
        *self.status.lock().unwrap() = status;
    }

    fn received(&self) -> Vec<(HeaderMap, Vec<u8>)> {
        self.received.lock().unwrap().clone()
    }
}

/// A browser's side of a push subscription: its key and secret.
struct Browser {
    private: PrivateKey,
    auth: [u8; 16],
    endpoint: String,
}

impl Browser {
    fn at(endpoint: &str) -> Self {
        Self {
            private: PrivateKey::generate(&ECDH_P256).unwrap(),
            auth: *b"sixteen octets!!",
            endpoint: endpoint.to_owned(),
        }
    }

    /// The device a browser PUTs: its name, and its subscription as
    /// `PushSubscription.toJSON()` writes it.
    fn device(&self) -> String {
        let public = self.private.compute_public_key().unwrap();
        json!({
            "name": "Firefox on a laptop",
            "wake": { "web_push": {
                "endpoint": self.endpoint,
                "keys": {
                    "p256dh": URL_SAFE_NO_PAD.encode(public.as_ref()),
                    "auth": URL_SAFE_NO_PAD.encode(self.auth),
                },
            } },
        })
        .to_string()
    }

    /// Decrypts a push this browser received.
    fn read(&self, body: &[u8]) -> Value {
        serde_json::from_slice(&decrypt(body, &self.private, &self.auth)).unwrap()
    }
}

async fn individual(app: &TestApp, name: &str) -> String {
    let token = app.issuer.token(&ulid::Ulid::generate().to_string());
    request(app, "PUT", &format!("/@{name}"), &token, &[], "").await;
    token
}

/// `@review-bot`, and `@chris`, who allowed it, with no devices yet.
struct Pair {
    app: TestApp,
    bot: String,
    chris: String,
}

async fn pair() -> Pair {
    let app = TestApp::new().await;
    let bot = individual(&app, "review-bot").await;
    let chris = individual(&app, "chris").await;
    request(&app, "POST", "/@chris", &bot, &[], "hello").await;
    decide(&app, &chris, "chris", "allow").await;
    Pair { app, bot, chris }
}

/// Subscribes `browser` as `@chris`'s device `laptop`.
async fn subscribe(pair: &Pair, browser: &Browser) -> StatusCode {
    request(
        &pair.app,
        "PUT",
        "/@chris/devices/laptop",
        &pair.chris,
        &[JSON],
        browser.device(),
    )
    .await
    .0
}

/// `@chris`'s devices and the key they subscribe with.
async fn devices(pair: &Pair) -> Value {
    let (_, _, text) = request(&pair.app, "GET", "/@chris/devices", &pair.chris, &[], "").await;
    serde_json::from_str(&text).unwrap()
}
