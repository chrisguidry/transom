//! The HTTP application: transom's shared state and its routes.

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::http::header::WWW_AUTHENTICATE;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::signin::Issuers;
use crate::store::Store;
use crate::{exchanges, health, identities, mcp, webhooks};

pub(crate) struct App {
    pub(crate) store: Store,
    pub(crate) issuers: Issuers,
    pub(crate) webhooks: crate::webhooks::Keys,
    pub(crate) http: reqwest::Client,
    /// The URL people reach transom at, if the settings give one.
    pub(crate) public_url: Option<String>,
}

impl App {
    pub(crate) fn new(store: Store, issuers: Issuers) -> Self {
        // reqwest's TLS needs a process-wide crypto provider. Installing it
        // a second time fails harmlessly.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        Self {
            store,
            issuers,
            webhooks: crate::webhooks::Keys::new(),
            http: reqwest::Client::new(),
            public_url: None,
        }
    }

    pub(crate) fn reached_at(mut self, public_url: Option<String>) -> Self {
        self.public_url = public_url.map(|url| url.trim_end_matches('/').to_owned());
        self
    }
}

pub(crate) fn router(app: Arc<App>) -> Router {
    Router::new()
        .merge(health::router())
        .merge(identities::router())
        .merge(exchanges::router())
        .merge(webhooks::router())
        .merge(crate::signin::router())
        .merge(crate::web::router())
        .merge(mcp::router(app.clone()))
        .layer(axum::middleware::from_fn(crate::web::pages_layer))
        .with_state(app)
}

/// A request that failed: its status and the reason, which the response
/// carries as JSON, and the `WWW-Authenticate` challenge of a 401.
#[derive(Debug)]
pub(crate) struct Failure {
    status: StatusCode,
    reason: String,
    challenge: Option<String>,
    detail: Option<(&'static str, serde_json::Value)>,
}

impl Failure {
    pub(crate) fn new(status: StatusCode, reason: impl std::fmt::Display) -> Self {
        Self {
            status,
            reason: reason.to_string(),
            challenge: None,
            detail: None,
        }
    }

    /// Adds a field to the failure's JSON body, such as the reply that won
    /// over a later one.
    pub(crate) fn with(mut self, name: &'static str, value: serde_json::Value) -> Self {
        self.detail = Some((name, value));
        self
    }

    pub(crate) fn challenging(mut self, challenge: String) -> Self {
        self.challenge = Some(challenge);
        self
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl From<redis::RedisError> for Failure {
    fn from(error: redis::RedisError) -> Self {
        tracing::error!(%error, "Redis failed");
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "transom's storage is unavailable",
        )
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let mut body = json!({ "error": self.reason });
        if let Some((name, value)) = self.detail {
            body[name] = value;
        }
        let body = Json(body);
        match self.challenge {
            Some(challenge) => (self.status, [(WWW_AUTHENTICATE, challenge)], body).into_response(),
            None => (self.status, body).into_response(),
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{HeaderMap, Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::{App, router};
    use crate::signin::test_issuer::TestIssuer;
    use crate::signin::{Issuer, Issuers};
    use crate::store::Store;
    use crate::store::testing::store;

    /// transom with a store of its own on the test Redis, trusting one
    /// test issuer.
    pub(crate) struct TestApp {
        pub(crate) app: Arc<App>,
        pub(crate) issuer: TestIssuer,
    }

    impl TestApp {
        pub(crate) async fn new() -> Self {
            let store = store();
            crate::identities::records::register_transom(&store)
                .await
                .unwrap();
            Self::built(store, None).await
        }

        /// transom on `store`, as it is: for a Redis that is down, as the
        /// tests of an outage want, nothing is registered in it.
        pub(crate) async fn with_store(store: Store) -> Self {
            Self::built(store, None).await
        }

        /// transom as `new` makes it, reached at `public_url`.
        pub(crate) async fn reached_at(public_url: &str) -> Self {
            let store = store();
            crate::identities::records::register_transom(&store)
                .await
                .unwrap();
            Self::built(store, Some(public_url.to_owned())).await
        }

        async fn built(store: Store, public_url: Option<String>) -> Self {
            let issuer = TestIssuer::start().await;
            let issuers = Issuers::of(vec![Issuer::new(issuer.settings()).unwrap()]);
            let app = Arc::new(App::new(store, issuers).reached_at(public_url));
            Self { app, issuer }
        }
    }

    /// Sends a request with `headers` and a raw `body` through transom's
    /// router, as `token`, returning the status, headers, and body text.
    pub(crate) async fn request(
        app: &TestApp,
        method: &str,
        path: &str,
        token: &str,
        headers: &[(&str, &str)],
        body: impl Into<Body>,
    ) -> (StatusCode, HeaderMap, String) {
        let mut request = Request::builder().method(method).uri(path);
        if !token.is_empty() {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = router(app.app.clone())
            .oneshot(request.body(body.into()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
    }

    /// Opens an event stream at `path` as `token` and returns its first
    /// `count` events as (event name, data) pairs, or fewer if the stream
    /// ends or goes quiet for five seconds.
    pub(crate) async fn events(
        app: &TestApp,
        path: &str,
        token: &str,
        count: usize,
        during: impl std::future::Future<Output = ()>,
    ) -> Vec<(String, Value)> {
        let request = Request::get(path)
            .header("authorization", format!("Bearer {token}"))
            .header("accept", "text/event-stream")
            .body(Body::empty())
            .unwrap();
        let response = router(app.app.clone()).oneshot(request).await.unwrap();
        let mut body = response.into_body().into_data_stream();
        let reading = async {
            use futures_util::StreamExt;
            let mut text = String::new();
            let mut events = Vec::new();
            while events.len() < count {
                let Ok(Some(Ok(chunk))) =
                    tokio::time::timeout(std::time::Duration::from_secs(5), body.next()).await
                else {
                    break;
                };
                text.push_str(std::str::from_utf8(&chunk).unwrap());
                while let Some(end) = text.find("\n\n") {
                    let block: String = text.drain(..end + 2).collect();
                    let mut name = String::new();
                    let mut data = String::new();
                    for line in block.lines() {
                        if let Some(value) = line.strip_prefix("event:") {
                            name = value.trim().to_owned();
                        } else if let Some(value) = line.strip_prefix("data:") {
                            data.push_str(value.trim());
                        }
                    }
                    if !name.is_empty() {
                        events.push((
                            name,
                            serde_json::from_str(&data).unwrap_or(Value::String(data)),
                        ));
                    }
                }
            }
            events
        };
        let (events, ()) = tokio::join!(reading, during);
        events
    }

    /// Sends one request through transom's router, returning the status,
    /// the headers, and the body as text.
    pub(crate) async fn send(
        app: &TestApp,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, HeaderMap, String) {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let body = match body {
            Some(body) => {
                request = request.header("content-type", "application/json");
                Body::from(body.to_string())
            }
            None => Body::empty(),
        };
        let response = router(app.app.clone())
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
    }
}
