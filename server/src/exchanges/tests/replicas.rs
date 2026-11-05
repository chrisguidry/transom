//! Exchanges across several servers on one Redis: what one server writes
//! as another reads it, a server that stops, and a Redis that fails over.

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use super::{JSON, Pair, household, individual, newest, pair, question, send};
use crate::app::router;
use crate::app::testing::{TestApp, events, request};
use crate::store::failover::Failover;

/// The id of the identity named `name`.
async fn id_of(app: &TestApp, name: &str) -> String {
    let store = &app.app.store;
    let mut connection = store.connection().await.unwrap();
    redis::cmd("HGET")
        .arg(store.key("names"))
        .arg(name)
        .query_async(&mut connection)
        .await
        .unwrap()
}

/// How many exchanges identity `id` has sent, and how many webhooks are
/// queued, read in one transaction, as another server reads them.
async fn sent_and_queued(app: &TestApp, id: &str) -> (usize, usize) {
    let store = &app.app.store;
    let mut connection = store.connection().await.unwrap();
    redis::pipe()
        .atomic()
        .zcard(store.key(&format!("sent:{id}")))
        .zcard(store.key("deliveries"))
        .query_async(&mut connection)
        .await
        .unwrap()
}

/// What another server reads while identity `id` sends `sends` exchanges,
/// until it reads every one.
async fn watched(app: &TestApp, id: &str, sends: usize) -> Vec<(usize, usize)> {
    let other = app.replica();
    let mut seen = vec![sent_and_queued(&other, id).await];
    while seen.last().unwrap().0 < sends {
        seen.push(sent_and_queued(&other, id).await);
    }
    seen
}

#[tokio::test]
async fn no_server_sees_an_exchange_before_its_webhook_is_queued() {
    let app = TestApp::new().await;
    let chris = individual(&app, "chris").await;
    let webhook = r#"{"webhook": "https://example.com/hook"}"#;
    request(&app, "PATCH", "/@chris", &chris, &[JSON], webhook).await;
    let id = id_of(&app, "chris").await;
    let sending = async {
        for _ in 0..20 {
            send(&app, &chris, "/@chris", &[], "Note to self").await;
        }
    };
    let ((), seen) = tokio::join!(sending, watched(&app, &id, 20));
    assert!(seen.iter().all(|(sent, queued)| sent == queued), "{seen:?}");
}

/// Whether the stream at `path` ends within two seconds of the server
/// stopping.
async fn ends_when_stopping(app: &TestApp, path: &str, token: &str) -> bool {
    let request = Request::get(path)
        .header("authorization", format!("Bearer {token}"))
        .header("accept", "text/event-stream")
        .body(Body::empty())
        .unwrap();
    let response = router(app.app.clone()).oneshot(request).await.unwrap();
    app.app.stopping.cancel();
    tokio::time::timeout(Duration::from_secs(2), response.into_body().collect())
        .await
        .is_ok()
}

#[tokio::test]
async fn a_live_feed_ends_when_the_server_stops() {
    let pair = pair().await;
    assert!(ends_when_stopping(&pair.app, "/@chris/exchanges", &pair.recipient).await);
}

#[tokio::test]
async fn an_exchanges_stream_ends_when_the_server_stops() {
    let pair = pair().await;
    let id = question(&pair).await;
    let path = format!("/exchanges/{id}");
    assert!(ends_when_stopping(&pair.app, &path, &pair.sender).await);
}

#[tokio::test]
async fn a_waiting_send_returns_when_the_server_stops() {
    let pair = pair().await;
    pair.app.app.stopping.cancel();
    let waiting = send(
        &pair.app,
        &pair.sender,
        "/@chris",
        &[JSON, ("prefer", "wait=600")],
        r#"{"summary": "OK?", "choices": ["Yes"]}"#,
    );
    let (status, _) = tokio::time::timeout(Duration::from_secs(2), waiting)
        .await
        .unwrap();
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn the_maintenance_pass_ends_when_the_server_stops() {
    let app = TestApp::new().await;
    app.app.stopping.cancel();
    let pass = crate::exchanges::expire(app.app.clone());
    assert!(
        tokio::time::timeout(Duration::from_secs(2), pass)
            .await
            .is_ok()
    );
}

/// `@review-bot` and `@chris` on one server, and a second server that
/// reaches the same Redis through a failover.
async fn failing_over() -> (Pair, TestApp, Failover) {
    let pair = pair().await;
    let failover = Failover::start().await;
    let other = pair.app.replica_at(&failover.url);
    (pair, other, failover)
}

/// The summaries of the exchanges in `seen`.
fn summaries(seen: &[(String, Value)]) -> Vec<&str> {
    seen.iter()
        .map(|(_, exchange)| exchange["message"]["summary"].as_str().unwrap_or(""))
        .collect()
}

#[tokio::test]
async fn a_live_feed_sends_its_list_again_after_its_connection_drops() {
    let (pair, other, failover) = failing_over().await;
    let happening = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        failover.cut();
        tokio::time::sleep(Duration::from_millis(500)).await;
        question(&pair).await;
    };
    let seen = events(&other, "/@chris/exchanges", &pair.recipient, 3, happening).await;
    assert_eq!(
        summaries(&seen),
        [
            "@review-bot wants to send you messages",
            "",
            "Restart postgres?"
        ]
    );
}

#[tokio::test]
async fn a_waiting_send_hears_the_reply_after_its_connection_drops() {
    let (pair, other, failover) = failing_over().await;
    let waiting = send(
        &other,
        &pair.sender,
        "/@chris",
        &[JSON, ("prefer", "wait=10")],
        r#"{"summary": "OK?", "choices": ["Yes"]}"#,
    );
    let replying = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        failover.cut();
        tokio::time::sleep(Duration::from_millis(500)).await;
        let id = newest(&pair.app, &pair.recipient, "chris").await;
        let reply = format!("/exchanges/{id}/reply");
        request(&pair.app, "PUT", &reply, &pair.recipient, &[], "Yes").await;
    };
    let ((status, exchange), ()) = tokio::join!(waiting, replying);
    assert_eq!(
        (status, exchange["state"].as_str()),
        (StatusCode::OK, Some("replied"))
    );
}

#[tokio::test]
async fn a_live_feed_whose_groups_changed_while_it_was_down_ends() {
    let (app, chris, _) = household().await;
    let failover = Failover::start().await;
    let other = app.replica_at(&failover.url);
    let happening = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        failover.refuse();
        request(&app, "PUT", "/@band", &chris, &[], "").await;
        failover.recover();
        tokio::time::sleep(Duration::from_secs(1)).await;
        send(&app, &chris, "/@chris", &[], "Note to self").await;
    };
    let seen = events(&other, "/@chris/exchanges", &chris, 1, happening).await;
    assert_eq!(seen.len(), 0);
}
