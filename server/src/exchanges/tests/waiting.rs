use std::time::Duration;

use axum::http::StatusCode;
use serde_json::Value;

use super::{JSON, pair, question, send};
use crate::app::testing::{TestApp, events, request};
use crate::exchanges::expiry;
use crate::store::testing::store_at;

/// The id of the newest exchange `token` can see in `@{name}`'s list,
/// waiting for one to appear.
async fn newest(app: &TestApp, token: &str, name: &str) -> String {
    for _ in 0..100 {
        let (_, _, text) =
            request(app, "GET", &format!("/@{name}/exchanges"), token, &[], "").await;
        let list: Value = serde_json::from_str(&text).unwrap();
        if let Some(id) = list["exchanges"][0]["id"].as_str() {
            return id.to_owned();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no exchange appeared");
}

#[tokio::test]
async fn a_send_can_wait_for_the_reply() {
    let pair = pair().await;
    let sending = send(
        &pair.app,
        &pair.sender,
        "/@chris",
        &[JSON, ("prefer", "respond-async, wait=10")],
        r#"{"summary": "OK?", "choices": ["Yes"]}"#,
    );
    let replying = async {
        let id = newest(&pair.app, &pair.recipient, "chris").await;
        request(
            &pair.app,
            "PUT",
            &format!("/exchanges/{id}/reply"),
            &pair.recipient,
            &[JSON],
            r#"{"choice": "Yes"}"#,
        )
        .await;
    };
    let ((status, exchange), ()) = tokio::join!(sending, replying);
    assert_eq!(
        (status, exchange["state"].as_str()),
        (StatusCode::OK, Some("replied"))
    );
}

#[tokio::test]
async fn a_send_whose_wait_runs_out_is_accepted() {
    let pair = pair().await;
    let (status, headers, _) = request(
        &pair.app,
        "POST",
        "/@chris?wait=1",
        &pair.sender,
        &[],
        "anyone?",
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(
        headers["location"]
            .to_str()
            .unwrap()
            .starts_with("/exchanges/")
    );
}

#[tokio::test]
async fn a_wait_that_is_not_a_duration_is_a_bad_request() {
    let pair = pair().await;
    let (status, _) = send(
        &pair.app,
        &pair.sender,
        "/@chris",
        &[("prefer", "wait=forever")],
        "hi",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn waiting_on_the_reply_returns_it_when_it_lands() {
    let pair = pair().await;
    let id = question(&pair).await;
    let path = format!("/exchanges/{id}/reply");
    let waiting = request(
        &pair.app,
        "GET",
        &path,
        &pair.sender,
        &[("prefer", "wait=10")],
        "",
    );
    let replying = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        request(&pair.app, "PUT", &path, &pair.recipient, &[], "sure").await;
    };
    let ((status, _, text), ()) = tokio::join!(waiting, replying);
    assert_eq!(status, StatusCode::OK);
    assert!(text.contains("sure"));
}

#[tokio::test]
async fn waiting_on_the_reply_ends_with_the_exchange() {
    let pair = pair().await;
    let id = question(&pair).await;
    request(
        &pair.app,
        "DELETE",
        &format!("/exchanges/{id}/message"),
        &pair.sender,
        &[],
        "",
    )
    .await;
    let (status, _, text) = request(
        &pair.app,
        "GET",
        &format!("/exchanges/{id}/reply?wait=10"),
        &pair.sender,
        &[],
        "",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(text.contains("withdrawn"));
}

#[tokio::test]
async fn streaming_the_exchange_follows_it_to_the_end() {
    let pair = pair().await;
    let id = question(&pair).await;
    let replying = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        request(
            &pair.app,
            "PUT",
            &format!("/exchanges/{id}/reply"),
            &pair.recipient,
            &[],
            "ok",
        )
        .await;
    };
    let seen = events(
        &pair.app,
        &format!("/exchanges/{id}"),
        &pair.sender,
        3,
        replying,
    )
    .await;
    let states: Vec<_> = seen
        .iter()
        .map(|(_, exchange)| exchange["state"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(states, ["open", "replied"]);
}

#[tokio::test]
async fn streaming_the_reply_sends_just_the_reply() {
    let pair = pair().await;
    let id = question(&pair).await;
    let replying = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        request(
            &pair.app,
            "PUT",
            &format!("/exchanges/{id}/reply"),
            &pair.recipient,
            &[],
            "ok",
        )
        .await;
    };
    let seen = events(
        &pair.app,
        &format!("/exchanges/{id}/reply"),
        &pair.sender,
        2,
        replying,
    )
    .await;
    assert_eq!(seen.len(), 1);
    assert_eq!(
        (seen[0].0.as_str(), seen[0].1["text"].as_str()),
        ("reply", Some("ok"))
    );
}

#[tokio::test]
async fn streaming_the_reply_ends_when_the_exchange_does() {
    let pair = pair().await;
    let id = question(&pair).await;
    let withdrawing = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        request(
            &pair.app,
            "DELETE",
            &format!("/exchanges/{id}/message"),
            &pair.sender,
            &[],
            "",
        )
        .await;
    };
    let seen = events(
        &pair.app,
        &format!("/exchanges/{id}/reply"),
        &pair.sender,
        2,
        withdrawing,
    )
    .await;
    assert_eq!(
        (seen[0].0.as_str(), seen[0].1["state"].as_str()),
        ("ended", Some("withdrawn"))
    );
}

#[tokio::test]
async fn an_exchange_expires_when_its_time_to_live_passes() {
    let pair = pair().await;
    let (_, exchange) = send(
        &pair.app,
        &pair.sender,
        "/@chris",
        &[JSON],
        r#"{"summary": "Now?", "ttl": 1}"#,
    )
    .await;
    let id = exchange["id"].as_str().unwrap();
    let later = crate::exchanges::records::now() + 2000;
    assert_eq!(expiry::expire_due(&pair.app.app, later).await.unwrap(), 1);
    assert_eq!(expiry::expire_due(&pair.app.app, later).await.unwrap(), 0);
    let (_, _, text) = request(
        &pair.app,
        "GET",
        &format!("/exchanges/{id}"),
        &pair.sender,
        &[],
        "",
    )
    .await;
    assert!(text.contains("\"expired\""));
}

#[tokio::test]
async fn a_pass_logs_a_redis_outage_and_carries_on() {
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("redis://{}", closed.local_addr().unwrap());
    drop(closed);
    let app = TestApp::with_store(store_at(&url)).await;
    expiry::tick(&app.app).await;
}

#[tokio::test]
async fn a_stale_expiry_is_dropped_without_announcing_anything() {
    let pair = pair().await;
    let mut connection = pair.app.app.store.connection().await.unwrap();
    redis::cmd("ZADD")
        .arg(pair.app.app.store.key("expiries"))
        .arg(1)
        .arg("01STRAY")
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    let later = crate::exchanges::records::now();
    assert_eq!(expiry::expire_due(&pair.app.app, later).await.unwrap(), 0);
    let left: Vec<String> = redis::cmd("ZRANGE")
        .arg(pair.app.app.store.key("expiries"))
        .arg(0)
        .arg(-1)
        .query_async(&mut connection)
        .await
        .unwrap();
    assert_eq!(left, Vec::<String>::new());
}
