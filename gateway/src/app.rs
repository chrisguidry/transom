//! The HTTP application: the gateway's state and its two routes.

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::http::header::RETRY_AFTER;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;

use crate::credentials::Credentials;
use crate::limits::Limiter;

pub(crate) struct App {
    pub(crate) http: reqwest::Client,
    /// The service account the gateway signs in as, or `None` before the
    /// Firebase project exists.
    pub(crate) credentials: Option<Credentials>,
    pub(crate) fcm_url: String,
    pub(crate) limiter: Mutex<Limiter>,
}

impl App {
    pub(crate) fn new(credentials: Option<Credentials>, fcm_url: &str) -> Self {
        // reqwest's TLS needs a process-wide crypto provider. Installing it
        // a second time fails harmlessly.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        Self {
            http: reqwest::Client::new(),
            credentials,
            fcm_url: fcm_url.trim_end_matches('/').to_owned(),
            limiter: Mutex::new(Limiter::new()),
        }
    }
}

pub(crate) fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .merge(crate::wake::router())
        .layer(axum::middleware::from_fn(crate::logs::requests))
        .with_state(app)
}

/// The gateway is ready whenever it answers, with or without credentials:
/// without them it still answers each wake-up, with 503.
async fn healthz() -> &'static str {
    "ok\n"
}

/// A request that failed: its status and the reason, which the response
/// carries as JSON, and how long to wait before asking again, when that is
/// known.
#[derive(Debug)]
pub(crate) struct Failure {
    status: StatusCode,
    reason: String,
    retry_after: Option<String>,
}

impl Failure {
    pub(crate) fn new(status: StatusCode, reason: impl std::fmt::Display) -> Self {
        Self {
            status,
            reason: reason.to_string(),
            retry_after: None,
        }
    }

    pub(crate) fn retry_after(mut self, retry_after: Option<String>) -> Self {
        self.retry_after = retry_after;
        self
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let body = Json(json!({ "error": self.reason }));
        let mut response = match self.retry_after {
            Some(seconds) => (self.status, [(RETRY_AFTER, seconds)], body).into_response(),
            None => (self.status, body).into_response(),
        };
        response
            .extensions_mut()
            .insert(crate::logs::Reason(self.reason));
        response
    }
}
