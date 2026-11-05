//! `POST /wake`: a server asks the gateway to wake one phone.
//!
//! - `{"token": "<FCM registration token>"}`, and nothing else, as JSON of
//!   at most 4 KiB.
//! - `202 Accepted` when FCM took the wake-up.
//! - `404 Not Found` when FCM will never deliver to the token, so the
//!   server can forget the device.
//! - `429 Too Many Requests` when the token has had its wake-ups for now,
//!   with `Retry-After`.
//! - `503 Service Unavailable` when the gateway has no credentials, or
//!   FCM is busy or failing, with `Retry-After` when FCM gave one.
//! - `502 Bad Gateway` when FCM refused the wake-up for another reason.

use std::sync::Arc;
use std::time::{Instant, SystemTime};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::routing::post;
use serde::Deserialize;

use crate::app::{App, Failure};
use crate::fcm::{self, Sent};
use crate::limits::fingerprint;

/// The largest body the gateway reads. A registration token is a few
/// hundred characters.
const LARGEST_BODY: usize = 4096;

pub(crate) fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/wake", post(wake))
        .layer(DefaultBodyLimit::max(LARGEST_BODY))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wake {
    token: String,
}

async fn wake(State(app): State<Arc<App>>, body: Bytes) -> Result<StatusCode, Failure> {
    let Ok(Wake { token }) = serde_json::from_slice(&body) else {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "the body must be {\"token\": \"<FCM registration token>\"}",
        ));
    };
    if token.is_empty() {
        return Err(Failure::new(StatusCode::BAD_REQUEST, "the token is empty"));
    }
    let device = fingerprint(&token);
    tracing::Span::current().record("device", tracing::field::display(&device));
    let Some(credentials) = &app.credentials else {
        return Err(Failure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "this gateway has no Firebase credentials",
        ));
    };
    let taken = app.limiter.lock().unwrap().take(&device, Instant::now());
    if let Err(wait) = taken {
        return Err(Failure::new(
            StatusCode::TOO_MANY_REQUESTS,
            "this token has been woken too often",
        )
        .retry_after(Some(wait.as_millis().div_ceil(1000).to_string())));
    }
    let access_token = credentials
        .access_token(&app.http, SystemTime::now())
        .await
        .map_err(|error| {
            tracing::error!(%error, "the gateway could not sign in to FCM");
            Failure::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "the gateway could not sign in to FCM",
            )
        })?;
    let sent = fcm::send(
        &app.http,
        &app.fcm_url,
        &credentials.project,
        &access_token,
        &token,
    )
    .await;
    match sent {
        Sent::Taken => Ok(StatusCode::ACCEPTED),
        Sent::Gone => Err(Failure::new(
            StatusCode::NOT_FOUND,
            "FCM no longer delivers to this token",
        )),
        Sent::Busy(retry_after) => Err(Failure::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "FCM is unavailable",
        )
        .retry_after(retry_after)),
        Sent::Unauthorized => {
            credentials.forget(&access_token).await;
            Err(Failure::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "FCM refused the gateway's access token",
            ))
        }
        Sent::Refused(reason) => {
            tracing::error!(reason, "FCM refused a wake-up");
            Err(Failure::new(StatusCode::BAD_GATEWAY, reason))
        }
    }
}
