//! Alerts in the browser: the urgency and tag a notification shows with,
//! and the repeats of an urgent message nobody has read.

use serde_json::{Value, json};

use super::{Browser, JSON, Pair, PushService, subscribed};
use crate::app::testing::request;
use crate::exchanges::records::now;
use crate::exchanges::repeat_due;
use crate::push::deliver_due;

const MINUTE: i64 = 60 * 1000;

/// Sends `@chris` `message` from `@review-bot`, and returns its id.
async fn send(pair: &Pair, message: Value) -> String {
    let body = message.to_string();
    let (_, _, text) = request(&pair.app, "POST", "/@chris", &pair.bot, &[JSON], body).await;
    serde_json::from_str::<Value>(&text).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Delivers what is due at `at`, and returns each push's `Urgency` header
/// and payload.
async fn delivered(
    pair: &Pair,
    service: &PushService,
    browser: &Browser,
    at: i64,
) -> Vec<(String, Value)> {
    deliver_due(&pair.app.app, at).await.unwrap();
    service
        .received()
        .iter()
        .map(|(headers, body)| {
            let urgency = headers["urgency"].to_str().unwrap().to_owned();
            (urgency, browser.read(body))
        })
        .collect()
}

#[tokio::test]
async fn a_very_low_message_is_never_pushed_to_a_browser() {
    let (pair, service, browser) = subscribed().await;
    let id = send(&pair, json!({ "summary": "Synced", "urgency": "very-low" })).await;
    let read = format!("/exchanges/{id}/@chris");
    request(&pair.app, "PUT", &read, &pair.chris, &[], "").await;
    let withdraw = format!("/exchanges/{id}/message");
    request(&pair.app, "DELETE", &withdraw, &pair.bot, &[], "").await;
    assert_eq!(delivered(&pair, &service, &browser, now()).await, vec![]);
}

#[tokio::test]
async fn an_urgent_message_is_pushed_as_high_and_shown_as_urgent() {
    let (pair, service, browser) = subscribed().await;
    send(
        &pair,
        json!({ "summary": "Disk full", "urgency": "urgent" }),
    )
    .await;
    let (urgency, payload) = delivered(&pair, &service, &browser, now()).await.remove(0);
    assert_eq!(
        (urgency.as_str(), payload["urgency"].as_str()),
        ("high", Some("urgent"))
    );
}

#[tokio::test]
async fn a_keyed_message_is_tagged_by_its_sender_recipient_and_key() {
    let (pair, service, browser) = subscribed().await;
    send(&pair, json!({ "summary": "Disk full", "key": "disk" })).await;
    let (_, payload) = delivered(&pair, &service, &browser, now()).await.remove(0);
    let tag = payload["tag"].as_str().unwrap();
    assert_eq!((tag.split(':').count(), tag.ends_with(":disk")), (3, true));
}

/// Sends `@chris` an urgent message, and returns its id and when it was
/// sent.
async fn urgent(pair: &Pair) -> (String, i64) {
    let id = send(pair, json!({ "summary": "Disk full", "urgency": "urgent" })).await;
    (id, now())
}

#[tokio::test]
async fn an_unread_urgent_message_repeats_at_5_15_and_30_minutes() {
    let (pair, service, browser) = subscribed().await;
    let (id, sent) = urgent(&pair).await;
    let app = &pair.app.app;
    let repeated = [
        repeat_due(app, sent + 4 * MINUTE).await.unwrap(),
        repeat_due(app, sent + 5 * MINUTE).await.unwrap(),
        repeat_due(app, sent + 15 * MINUTE).await.unwrap(),
        repeat_due(app, sent + 30 * MINUTE).await.unwrap(),
        repeat_due(app, sent + 60 * MINUTE).await.unwrap(),
    ];
    let pushes = delivered(&pair, &service, &browser, sent + 60 * MINUTE).await;
    let ids: Vec<&str> = pushes
        .iter()
        .map(|(_, payload)| payload["id"].as_str().unwrap())
        .collect();
    assert_eq!((repeated, ids), ([0, 1, 1, 1, 0], vec![id.as_str(); 4]));
}

#[tokio::test]
async fn the_view_counts_the_repeats() {
    let (pair, _service, _browser) = subscribed().await;
    let (id, sent) = urgent(&pair).await;
    repeat_due(&pair.app.app, sent + 15 * MINUTE).await.unwrap();
    let path = format!("/exchanges/{id}");
    let (_, _, text) = request(&pair.app, "GET", &path, &pair.chris, &[], "").await;
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["repeats"], 2);
}

#[tokio::test]
async fn a_message_below_urgent_never_repeats() {
    let (pair, _service, _browser) = subscribed().await;
    send(&pair, json!({ "summary": "Disk full", "urgency": "high" })).await;
    let repeated = repeat_due(&pair.app.app, now() + 60 * MINUTE).await;
    assert_eq!(repeated.unwrap(), 0);
}

#[tokio::test]
async fn repeats_stop_once_the_message_is_read_answered_or_withdrawn() {
    let stops = [
        ("PUT", "@chris", true),
        ("PUT", "reply", true),
        ("DELETE", "message", false),
    ];
    for (method, leg, by_chris) in stops {
        let (pair, service, browser) = subscribed().await;
        let (id, sent) = urgent(&pair).await;
        let token = if by_chris { &pair.chris } else { &pair.bot };
        let path = format!("/exchanges/{id}/{leg}");
        request(&pair.app, method, &path, token, &[], "on it").await;
        let before = delivered(&pair, &service, &browser, now()).await.len();
        repeat_due(&pair.app.app, sent + 30 * MINUTE).await.unwrap();
        let after = delivered(&pair, &service, &browser, sent + 30 * MINUTE).await;
        assert_eq!(after.len(), before, "{method} {leg}");
    }
}
