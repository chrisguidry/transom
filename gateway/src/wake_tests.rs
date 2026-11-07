//! `POST /` and `GET /healthz` through the gateway's router, against
//! the stand-in for Google's endpoints.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

use crate::app::{App, router};
use crate::credentials::Credentials;
use crate::logs::testing::Captured;
use crate::stand_in::{PROJECT, StandIn};

/// A port that nothing listens on: bound to find a free one, then closed.
async fn closed_url() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}

/// The service account the stand-in's metadata server grants tokens for.
async fn credentials(stand_in: &StandIn) -> Credentials {
    Credentials::discover(&crate::app::http_client(), stand_in.url())
        .await
        .unwrap()
}

/// The gateway, signing in through the stand-in.
async fn gateway(stand_in: &StandIn) -> Arc<App> {
    Arc::new(App::new(Some(credentials(stand_in).await), stand_in.url()))
}

/// Sends `method path` with `body` through the gateway's router, returning
/// the status, the headers, and the body as text.
async fn request(
    app: &Arc<App>,
    method: &str,
    path: &str,
    body: impl Into<Body>,
) -> (StatusCode, HeaderMap, String) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(body.into())
        .unwrap();
    let response = router(app.clone()).oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

/// Asks the gateway to wake `token`.
async fn wake(app: &Arc<App>, token: &str) -> (StatusCode, HeaderMap, String) {
    request(app, "POST", "/", json!({ "token": token }).to_string()).await
}

#[tokio::test]
async fn a_wake_up_is_accepted() {
    let stand_in = StandIn::start().await;
    let (status, _, body) = wake(&gateway(&stand_in).await, "a-phone").await;
    assert_eq!((status, body.as_str()), (StatusCode::ACCEPTED, ""));
}

#[tokio::test]
async fn a_wake_up_is_a_data_message_with_nothing_in_it() {
    let stand_in = StandIn::start().await;
    wake(&gateway(&stand_in).await, "a-phone").await;
    assert_eq!(
        stand_in.messages(),
        vec![(
            PROJECT.to_owned(),
            json!({
                "message": {
                    "fid": "a-phone",
                    "data": { "wake": "1" },
                    "android": { "priority": "HIGH", "ttl": "900s", "collapse_key": "wake" },
                },
            })
        )]
    );
}

/// The status and the `Retry-After` of the gateway's answer when FCM
/// answers as the stand-in does for `token`.
async fn answer(token: &str) -> (StatusCode, Option<String>) {
    let stand_in = StandIn::start().await;
    let (status, headers, _) = wake(&gateway(&stand_in).await, token).await;
    let retry_after = headers
        .get("retry-after")
        .map(|value| value.to_str().unwrap().to_owned());
    (status, retry_after)
}

#[tokio::test]
async fn a_token_fcm_forgot_is_not_found() {
    assert_eq!(answer("unregistered").await, (StatusCode::NOT_FOUND, None));
}

#[tokio::test]
async fn a_token_fcm_finds_invalid_is_not_found() {
    assert_eq!(answer("invalid").await, (StatusCode::NOT_FOUND, None));
}

#[tokio::test]
async fn a_busy_fcm_is_unavailable_for_as_long_as_it_says() {
    assert_eq!(
        answer("busy").await,
        (StatusCode::SERVICE_UNAVAILABLE, Some("30".to_owned()))
    );
}

#[tokio::test]
async fn a_failing_fcm_is_unavailable() {
    assert_eq!(
        answer("failing").await,
        (StatusCode::SERVICE_UNAVAILABLE, None)
    );
}

#[tokio::test]
async fn a_refusal_from_fcm_is_a_bad_gateway() {
    assert_eq!(answer("foreign").await, (StatusCode::BAD_GATEWAY, None));
}

#[tokio::test]
async fn an_unreachable_fcm_is_unavailable() {
    let stand_in = StandIn::start().await;
    let app = Arc::new(App::new(
        Some(credentials(&stand_in).await),
        &closed_url().await,
    ));
    let (status, _, _) = wake(&app, "a-phone").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn without_credentials_every_wake_up_is_unavailable() {
    let app = Arc::new(App::new(None, &closed_url().await));
    let (status, _, body) = wake(&app, "a-phone").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body,
        r#"{"error":"this gateway has no Firebase credentials"}"#
    );
}

/// The status of the gateway's answer to `body`.
async fn status_of(body: &str) -> StatusCode {
    let app = Arc::new(App::new(None, &closed_url().await));
    request(&app, "POST", "/", body.to_owned()).await.0
}

#[tokio::test]
async fn a_body_that_is_not_json_is_a_bad_request() {
    assert_eq!(status_of("token=a-phone").await, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_body_without_a_token_is_a_bad_request() {
    assert_eq!(
        status_of(r#"{"to": "a-phone"}"#).await,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn a_body_with_more_than_a_token_is_a_bad_request() {
    let body = r#"{"token": "a-phone", "title": "hello"}"#;
    assert_eq!(status_of(body).await, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_empty_token_is_a_bad_request() {
    assert_eq!(status_of(r#"{"token": ""}"#).await, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_body_over_4_kib_is_too_large() {
    let body = json!({ "token": "a".repeat(4096) }).to_string();
    assert_eq!(status_of(&body).await, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn a_token_woken_too_often_is_told_to_wait() {
    let stand_in = StandIn::start().await;
    let app = gateway(&stand_in).await;
    for _ in 0..5 {
        wake(&app, "a-phone").await;
    }
    let (status, headers, _) = wake(&app, "a-phone").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(headers["retry-after"], "15");
    assert_eq!(stand_in.messages().len(), 5);
}

#[tokio::test]
async fn one_access_token_serves_many_wake_ups() {
    let stand_in = StandIn::start().await;
    let app = gateway(&stand_in).await;
    wake(&app, "a-phone").await;
    wake(&app, "another-phone").await;
    assert_eq!(stand_in.grants(), 1);
}

#[tokio::test]
async fn an_access_token_fcm_refuses_is_traded_for_another() {
    let stand_in = StandIn::start().await;
    let app = gateway(&stand_in).await;
    wake(&app, "a-phone").await;
    stand_in.revoke();
    let (refused, _, _) = wake(&app, "a-phone").await;
    let (accepted, _, _) = wake(&app, "a-phone").await;
    assert_eq!(
        (refused, accepted, stand_in.grants()),
        (StatusCode::SERVICE_UNAVAILABLE, StatusCode::ACCEPTED, 2)
    );
}

#[tokio::test]
async fn a_metadata_server_that_grants_no_token_is_unavailable() {
    let stand_in = StandIn::start().await;
    let app = gateway(&stand_in).await;
    stand_in.refuse_tokens();
    let (status, _, _) = wake(&app, "a-phone").await;
    assert_eq!(
        (status, stand_in.grants()),
        (StatusCode::SERVICE_UNAVAILABLE, 0)
    );
}

#[tokio::test]
async fn the_health_check_answers_without_credentials() {
    let app = Arc::new(App::new(None, &closed_url().await));
    let (status, _, body) = request(&app, "GET", "/healthz", "").await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "ok\n"));
}

#[tokio::test]
async fn a_wake_up_is_logged_by_fingerprint_and_never_by_token() {
    let stand_in = StandIn::start().await;
    let app = gateway(&stand_in).await;
    let logs = Captured::start();
    wake(&app, "a-secret-token").await;
    let line = logs.answered();
    let device = crate::limits::fingerprint("a-secret-token");
    assert!(
        line.contains(&format!(
            "request{{method=POST path=/ route=/ device={device}}}"
        )),
        "{line}"
    );
    assert!(line.contains("status=202"), "{line}");
    assert!(!logs.all().contains("a-secret-token"), "{}", logs.all());
}

#[tokio::test]
async fn a_failure_logs_its_reason_as_a_warning() {
    let app = Arc::new(App::new(None, &closed_url().await));
    let logs = Captured::start();
    wake(&app, "a-phone").await;
    let line = logs.answered();
    assert!(line.starts_with(" WARN request{"), "{line}");
    assert!(
        line.contains("reason=\"this gateway has no Firebase credentials\""),
        "{line}"
    );
}

#[tokio::test]
async fn a_refused_request_is_logged_as_information() {
    let app = Arc::new(App::new(None, &closed_url().await));
    let logs = Captured::start();
    request(&app, "POST", "/", "{}").await;
    assert!(
        logs.answered().starts_with(" INFO request{"),
        "{}",
        logs.all()
    );
}

#[tokio::test]
async fn health_checks_are_logged_only_for_debugging() {
    let app = Arc::new(App::new(None, &closed_url().await));
    let logs = Captured::start();
    request(&app, "GET", "/healthz", "").await;
    assert!(
        logs.answered().starts_with("DEBUG request{"),
        "{}",
        logs.all()
    );
}
