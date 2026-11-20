//! What `/metrics` counts of the exchanges sent and how each ended.

use axum::http::StatusCode;
use serde_json::json;

use super::{JSON, decide, individual, pair, question, send};
use crate::app::testing::{TestApp, request};
use crate::exchanges::expiry;
use crate::metrics::testing::{sample, scrape};

/// How many `series` counts on `app`'s `/metrics`, or none when it has no
/// sample yet.
async fn counted(app: &TestApp, series: &str) -> u64 {
    let text = scrape(app).await;
    sample(&text, series).map_or(0, |value| value.parse().unwrap())
}

#[tokio::test]
async fn counts_what_is_sent_by_kind_issuer_and_how() {
    let pair = pair().await;
    question(&pair).await;
    let notification =
        "transom_exchanges_created_total{kind=\"notification\",issuer=\"test\",via=\"http\"}";
    let asked = "transom_exchanges_created_total{kind=\"question\",issuer=\"test\",via=\"http\"}";
    assert_eq!(
        (
            counted(&pair.app, notification).await,
            counted(&pair.app, asked).await
        ),
        (1, 1)
    );
}

const REPLIED: &str = "transom_exchanges_ended_total{kind=\"question\",state=\"replied\"}";

#[tokio::test]
async fn counts_a_reply() {
    let pair = pair().await;
    let id = question(&pair).await;
    let before = counted(&pair.app, REPLIED).await;
    let path = format!("/exchanges/{id}/reply");
    let body = r#"{"choice": "restart"}"#;
    let (status, _, _) = request(&pair.app, "PUT", &path, &pair.recipient, &[JSON], body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(counted(&pair.app, REPLIED).await - before, 1);
}

#[tokio::test]
async fn counts_a_withdrawal() {
    let pair = pair().await;
    let id = question(&pair).await;
    let path = format!("/exchanges/{id}/message");
    request(&pair.app, "DELETE", &path, &pair.sender, &[], "").await;
    request(&pair.app, "DELETE", &path, &pair.sender, &[], "").await;
    let series = "transom_exchanges_ended_total{kind=\"question\",state=\"withdrawn\"}";
    assert_eq!(counted(&pair.app, series).await, 1);
}

#[tokio::test]
async fn counts_an_expiry() {
    let pair = pair().await;
    let body = r#"{"summary": "Now?", "ttl": 1}"#;
    send(&pair.app, &pair.sender, "/@chris", &[JSON], body).await;
    let later = crate::exchanges::records::now() + 2000;
    expiry::expire_due(&pair.app.app, later).await.unwrap();
    let series = "transom_exchanges_ended_total{kind=\"notification\",state=\"expired\"}";
    assert_eq!(counted(&pair.app, series).await, 1);
}

#[tokio::test]
async fn counts_a_declined_message() {
    let app = TestApp::new().await;
    let sender = individual(&app, "review-bot").await;
    let recipient = individual(&app, "chris").await;
    send(&app, &sender, "/@chris", &[], "hello").await;
    decide(&app, &recipient, "chris", "decline").await;
    let series = "transom_exchanges_ended_total{kind=\"notification\",state=\"declined\"}";
    assert_eq!(counted(&app, series).await, 1);
}

#[tokio::test]
async fn counts_a_replaced_message() {
    let pair = pair().await;
    let body = json!({ "summary": "Disk is filling", "key": "disk" }).to_string();
    send(&pair.app, &pair.sender, "/@chris", &[JSON], &body).await;
    send(&pair.app, &pair.sender, "/@chris", &[JSON], &body).await;
    let series = "transom_exchanges_ended_total{kind=\"notification\",state=\"replaced\"}";
    assert_eq!(counted(&pair.app, series).await, 1);
}

#[tokio::test]
async fn a_pass_that_fails_is_counted() {
    // Nothing listens on port 1, so every use of Redis fails.
    let app = TestApp::with_store(crate::store::testing::store_at("redis://127.0.0.1:1")).await;
    expiry::tick(&app.app).await;
    let text = scrape(&app).await;
    assert_eq!(
        sample(&text, "transom_maintenance_failures_total"),
        Some("1")
    );
}
