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
use crate::{health, identities};

pub(crate) struct App {
    pub(crate) store: Store,
    pub(crate) issuers: Issuers,
}

pub(crate) fn router(app: Arc<App>) -> Router {
    Router::new()
        .merge(health::router())
        .merge(identities::router())
        .with_state(app)
}

/// A request that failed: its status and the reason, which the response
/// carries as JSON, and the `WWW-Authenticate` challenge of a 401.
#[derive(Debug)]
pub(crate) struct Failure {
    status: StatusCode,
    reason: String,
    challenge: Option<String>,
}

impl Failure {
    pub(crate) fn new(status: StatusCode, reason: impl std::fmt::Display) -> Self {
        Self {
            status,
            reason: reason.to_string(),
            challenge: None,
        }
    }

    pub(crate) fn challenging(mut self, challenge: String) -> Self {
        self.challenge = Some(challenge);
        self
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
        let body = Json(json!({ "error": self.reason }));
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
            Self::with_store(store()).await
        }

        pub(crate) async fn with_store(store: Store) -> Self {
            let issuer = TestIssuer::start().await;
            let issuers = Issuers::of(vec![Issuer::new(issuer.settings()).unwrap()]);
            let app = Arc::new(App { store, issuers });
            Self { app, issuer }
        }
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
