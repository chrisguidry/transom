//! Event streams opened with a session close when it ends, whether its
//! end is announced or found at the stream's next check of it.

use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use redis::AsyncCommands;
use tower::ServiceExt;

use super::send;
use crate::app::router;
use crate::app::testing::{TestApp, events, request};
use crate::signin::SESSION_PREFIX;
use crate::signin::sessions::{self, Kind, SignedIn};
use crate::store::failover::Failover;

/// A session of the app's for a new person, `@pixel-owner`, as its bearer
/// token, and the id of a note to self it sent.
async fn signed_in(app: &TestApp) -> (String, String) {
    let person = SignedIn {
        issuer: app.issuer.url.clone(),
        subject: ulid::Ulid::generate().to_string(),
        name: "pixel-owner".to_owned(),
        refresh: String::new(),
    };
    let token = sessions::start(&app.app, &person, Kind::App).await.unwrap();
    let bearer = format!("{SESSION_PREFIX}{token}");
    request(app, "PUT", "/@pixel-owner", &bearer, &[], "").await;
    let (_, note) = send(app, &bearer, "/@pixel-owner", &[], "Note to self").await;
    (bearer, note["id"].as_str().unwrap().to_owned())
}

/// Whether the stream at `path`, opened with `token`, ends within two
/// seconds of `ending`.
async fn ends_after(
    app: &TestApp,
    path: &str,
    token: &str,
    ending: impl Future<Output = ()>,
) -> bool {
    let request = Request::get(path)
        .header("authorization", format!("Bearer {token}"))
        .header("accept", "text/event-stream")
        .body(Body::empty())
        .unwrap();
    let response = router(app.app.clone()).oneshot(request).await.unwrap();
    ending.await;
    tokio::time::timeout(Duration::from_secs(2), response.into_body().collect())
        .await
        .is_ok()
}

/// Signs `token` out.
async fn sign_out(app: &TestApp, token: &str) {
    request(app, "POST", "/logout", token, &[], "").await;
}

/// Deletes the session behind `token`, as Redis does when it expires.
async fn expire(app: &TestApp, token: &str) {
    let session = token.strip_prefix(SESSION_PREFIX).unwrap();
    let key = app
        .app
        .store
        .key(&format!("session:{}", sessions::hashed(session)));
    let mut connection = app.app.store.connection().await.unwrap();
    let () = connection.del(key).await.unwrap();
}

#[tokio::test]
async fn a_live_feed_closes_when_its_session_signs_out() {
    let app = TestApp::new().await;
    let (token, _) = signed_in(&app).await;
    let path = "/@pixel-owner/exchanges";
    assert!(ends_after(&app, path, &token, sign_out(&app, &token)).await);
}

#[tokio::test]
async fn an_exchanges_changes_close_when_its_session_signs_out() {
    let app = TestApp::new().await;
    let (token, id) = signed_in(&app).await;
    let path = format!("/exchanges/{id}?changes");
    assert!(ends_after(&app, &path, &token, sign_out(&app, &token)).await);
}

#[tokio::test]
async fn a_reply_stream_closes_when_its_session_signs_out() {
    let app = TestApp::new().await;
    let (token, id) = signed_in(&app).await;
    let path = format!("/exchanges/{id}/reply");
    assert!(ends_after(&app, &path, &token, sign_out(&app, &token)).await);
}

#[tokio::test]
async fn a_live_feed_closes_at_its_next_check_after_its_session_expires() {
    let app = TestApp::checking_sessions_every(Duration::from_millis(100)).await;
    let (token, _) = signed_in(&app).await;
    let path = "/@pixel-owner/exchanges";
    assert!(ends_after(&app, path, &token, expire(&app, &token)).await);
}

#[tokio::test]
async fn a_live_feed_stays_open_while_its_session_goes_on() {
    let app = TestApp::checking_sessions_every(Duration::from_millis(100)).await;
    let (token, _) = signed_in(&app).await;
    let path = "/@pixel-owner/exchanges";
    assert!(!ends_after(&app, path, &token, async {}).await);
}

#[tokio::test]
async fn a_live_feed_sends_changes_while_its_session_goes_on() {
    let app = TestApp::new().await;
    let (token, _) = signed_in(&app).await;
    let sending = async {
        send(&app, &token, "/@pixel-owner", &[], "Another note").await;
    };
    let seen = events(&app, "/@pixel-owner/exchanges", &token, 1, sending).await;
    assert_eq!(seen[0].0, "exchange");
}

#[tokio::test]
async fn a_live_feed_whose_connection_drops_goes_on_while_its_session_does() {
    let app = TestApp::new().await;
    let (token, _) = signed_in(&app).await;
    let failover = Failover::start().await;
    let other = app.replica_at(&failover.url);
    let happening = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        failover.cut();
        tokio::time::sleep(Duration::from_millis(500)).await;
        send(&app, &token, "/@pixel-owner", &[], "Another note").await;
    };
    let seen = events(&other, "/@pixel-owner/exchanges", &token, 1, happening).await;
    assert_eq!(seen[0].0, "exchange");
}

#[tokio::test]
async fn a_live_feed_stays_open_while_redis_cannot_answer_its_session_check() {
    let app = TestApp::new().await;
    let (token, _) = signed_in(&app).await;
    let failover = Failover::start().await;
    let other = app.replica_checking_sessions_every(&failover.url, Duration::from_millis(100));
    let outage = async {
        // The feed's first check runs once it is read, and needs Redis.
        tokio::time::sleep(Duration::from_millis(300)).await;
        failover.go_silent();
        tokio::time::sleep(Duration::from_secs(3)).await;
        failover.recover();
    };
    let request = Request::get("/@pixel-owner/exchanges")
        .header("authorization", format!("Bearer {token}"))
        .header("accept", "text/event-stream")
        .body(Body::empty())
        .unwrap();
    let response = router(other.app.clone()).oneshot(request).await.unwrap();
    let reading = tokio::time::timeout(Duration::from_secs(5), response.into_body().collect());
    let (read, ()) = tokio::join!(reading, outage);
    assert!(read.is_err(), "the feed ended");
}
