//! What `/metrics` reports about the server itself: its build, the
//! requests it answers, its queues, and its use of Redis.

use axum::body::Body;
use axum::http::Request;
use axum::http::header::CONTENT_TYPE;
use redis::AsyncCommands;
use tower::ServiceExt;

use super::testing::{sample, scrape};
use crate::app::testing::{TestApp, send};
use crate::store::failover::Failover;
use crate::store::testing::{store, store_at};

#[tokio::test]
async fn reports_the_build_and_ends_as_openmetrics_does() {
    let app = TestApp::new().await;
    let text = scrape(&app).await;
    assert_eq!(
        sample(&text, "transom_build_info{version=\"dev\"}"),
        Some("1")
    );
    assert!(text.ends_with("# EOF\n"), "{text}");
}

#[tokio::test]
async fn is_served_as_openmetrics_text() {
    let app = TestApp::new().await;
    let request = Request::get("/metrics").body(Body::empty()).unwrap();
    let response = super::scrape::router(app.app.clone())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(
        response.headers()[CONTENT_TYPE],
        "application/openmetrics-text; version=1.0.0; charset=utf-8"
    );
}

#[tokio::test]
async fn counts_a_request_by_its_route_and_status() {
    let app = TestApp::new().await;
    send(&app, "GET", "/healthz", None, None).await;
    let text = scrape(&app).await;
    assert_eq!(
        sample(
            &text,
            "transom_http_requests_total{method=\"GET\",route=\"/healthz\",status=\"200\"}"
        ),
        Some("1")
    );
    assert_eq!(
        sample(
            &text,
            "transom_http_request_duration_seconds_count{method=\"GET\",route=\"/healthz\"}"
        ),
        Some("1")
    );
}

#[tokio::test]
async fn counts_a_request_no_route_matches_under_no_route() {
    let app = TestApp::new().await;
    let (status, _, _) = send(&app, "GET", "/nowhere/at/all", None, None).await;
    let text = scrape(&app).await;
    let series = format!(
        "transom_http_requests_total{{method=\"GET\",route=\"\",status=\"{}\"}}",
        status.as_u16()
    );
    assert_eq!(sample(&text, &series), Some("1"));
}

#[tokio::test]
async fn names_a_method_it_does_not_take_other() {
    let app = TestApp::new().await;
    for (method, label) in [
        ("HEAD", "HEAD"),
        ("POST", "POST"),
        ("PUT", "PUT"),
        ("DELETE", "DELETE"),
        ("PATCH", "PATCH"),
        ("OPTIONS", "OPTIONS"),
        ("BREW", "other"),
    ] {
        send(&app, method, "/healthz", None, None).await;
        let text = scrape(&app).await;
        let series = format!(
            "transom_http_request_duration_seconds_count{{method=\"{label}\",route=\"/healthz\"}}"
        );
        assert_eq!(sample(&text, &series), Some("1"), "{method}");
    }
}

#[tokio::test]
async fn reads_each_queue_from_redis() {
    let app = TestApp::with_store(store()).await;
    let mut connection = app.app.store.connection().await.unwrap();
    let due = crate::exchanges::records::now() - 5000;
    for (queue, key) in [
        ("webhook", "deliveries"),
        ("push", "pushes"),
        ("wake", "wakes"),
        ("repeat", "repeats"),
        ("expiry", "expiries"),
    ] {
        let () = connection
            .zadd(app.app.store.key(key), "late", due)
            .await
            .unwrap();
        let text = scrape(&app).await;
        let items = format!("transom_queue_items{{queue=\"{queue}\"}}");
        let lag = format!("transom_queue_lag_seconds{{queue=\"{queue}\"}}");
        assert_eq!(sample(&text, &items), Some("1"), "{queue}");
        let lag: f64 = sample(&text, &lag).unwrap().parse().unwrap();
        assert!(lag >= 5.0, "{queue}: {lag}");
    }
}

#[tokio::test]
async fn reports_an_empty_queue_as_on_time() {
    let app = TestApp::with_store(store()).await;
    let text = scrape(&app).await;
    assert_eq!(
        sample(&text, "transom_queue_items{queue=\"webhook\"}"),
        Some("0")
    );
    assert_eq!(
        sample(&text, "transom_queue_lag_seconds{queue=\"webhook\"}"),
        Some("0.0")
    );
}

#[tokio::test]
async fn leaves_out_the_queues_and_counts_the_error_while_redis_is_down() {
    // Nothing listens on port 1, so every use of Redis fails.
    let app = TestApp::with_store(store_at("redis://127.0.0.1:1")).await;
    let text = scrape(&app).await;
    assert!(!text.contains("transom_queue_items{"), "{text}");
    assert_eq!(
        sample(&text, "transom_redis_errors_total{kind=\"connection\"}"),
        Some("1")
    );
}

#[tokio::test]
async fn counts_a_command_redis_does_not_answer_in_time() {
    let failover = Failover::start().await;
    let app = TestApp::with_store(store_at(&failover.url)).await;
    let mut connection = app.app.store.connection().await.unwrap();
    failover.go_silent();
    let silent: redis::RedisResult<Option<String>> = connection.get("anything").await;
    failover.recover();
    let text = scrape(&app).await;
    assert!(silent.is_err());
    assert_eq!(
        sample(&text, "transom_redis_errors_total{kind=\"timeout\"}"),
        Some("1")
    );
}

#[tokio::test]
async fn counts_the_subscriptions_held_open() {
    let app = TestApp::with_store(store()).await;
    let subscription = app
        .app
        .store
        .subscribe(&["channel".to_owned()])
        .await
        .unwrap();
    let open = scrape(&app).await;
    drop(subscription);
    let closed = scrape(&app).await;
    assert_eq!(sample(&open, "transom_subscriptions"), Some("1"));
    assert_eq!(sample(&closed, "transom_subscriptions"), Some("0"));
}
