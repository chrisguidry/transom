//! Clearing many notifications at once: marking many exchanges read
//! reaches each browser as one push of their ids, and an exchange that
//! retention deletes closes its notification.

use serde_json::{Value, json};

use super::{Browser, JSON, Pair, PushService, pair, subscribe};
use crate::app::testing::request;
use crate::exchanges::records::now;
use crate::exchanges::retention;
use crate::push::deliver_due;

/// `@chris`, as `pair` makes him, with a browser subscribed.
async fn subscribed() -> (Pair, PushService, Browser) {
    let pair = pair().await;
    let service = PushService::start().await;
    let browser = Browser::at(&service.url);
    subscribe(&pair, &browser).await;
    (pair, service, browser)
}

/// Sends `@chris` `count` notifications from `@review-bot`, and returns
/// their ids, oldest first.
async fn notifications(pair: &Pair, count: usize) -> Vec<String> {
    let mut ids = Vec::new();
    for n in 0..count {
        let body = json!({ "body": format!("note {n}") }).to_string();
        ids.push(sent(pair, &body).await);
    }
    ids
}

/// Sends `@chris` the JSON message `body` from `@review-bot`, and returns
/// its id.
async fn sent(pair: &Pair, body: &str) -> String {
    let (_, _, text) = request(
        &pair.app,
        "POST",
        "/@chris",
        &pair.bot,
        &[JSON],
        body.to_owned(),
    )
    .await;
    serde_json::from_str::<Value>(&text).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Marks `ids` read as `@chris`, all at once.
async fn mark_read(pair: &Pair, ids: &[String]) {
    let body = json!({ "exchanges": ids }).to_string();
    request(
        &pair.app,
        "POST",
        "/@chris/read",
        &pair.chris,
        &[JSON],
        body,
    )
    .await;
}

/// Each push the browser has received, decrypted, once what is due is
/// delivered.
async fn pushed(pair: &Pair, service: &PushService, browser: &Browser) -> Vec<Value> {
    deliver_due(&pair.app.app, now()).await.unwrap();
    service
        .received()
        .iter()
        .map(|(_, body)| browser.read(body))
        .collect()
}

/// The ids that the read pushes among `pushes` clear, sorted, which for
/// exchange ids is oldest first. Pushes queued in the same millisecond
/// are delivered in any order.
fn cleared(pushes: &[Value]) -> Vec<String> {
    let mut cleared: Vec<String> = pushes
        .iter()
        .filter(|push| push["event"] == "read")
        .flat_map(|push| match push.get("ids") {
            Some(ids) => serde_json::from_value::<Vec<String>>(ids.clone()).unwrap(),
            None => vec![push["id"].as_str().unwrap().to_owned()],
        })
        .collect();
    cleared.sort();
    cleared
}

#[tokio::test]
async fn marking_many_read_pushes_their_ids_at_once() {
    let (pair, service, browser) = subscribed().await;
    let ids = notifications(&pair, 2).await;
    mark_read(&pair, &ids).await;
    let pushes = pushed(&pair, &service, &browser).await;
    let events: Vec<&str> = pushes
        .iter()
        .map(|push| push["event"].as_str().unwrap())
        .collect();
    assert_eq!(events, ["received", "received", "read"]);
    assert_eq!(pushes[2], json!({ "event": "read", "ids": ids }));
}

#[tokio::test]
async fn more_ids_than_one_push_holds_go_in_several() {
    let (pair, service, browser) = subscribed().await;
    let ids = notifications(&pair, 101).await;
    mark_read(&pair, &ids).await;
    let pushes = pushed(&pair, &service, &browser).await;
    let reads = pushes.iter().filter(|push| push["event"] == "read").count();
    assert_eq!((reads, cleared(&pushes)), (2, ids));
    let largest = service.received().iter().map(|(_, body)| body.len()).max();
    assert!(largest <= Some(4096), "{largest:?}");
}

#[tokio::test]
async fn marking_what_is_already_read_pushes_nothing() {
    let (pair, service, browser) = subscribed().await;
    let ids = notifications(&pair, 2).await;
    mark_read(&pair, &ids).await;
    mark_read(&pair, &ids).await;
    let pushes = pushed(&pair, &service, &browser).await;
    assert_eq!(cleared(&pushes), ids);
}

#[tokio::test]
async fn marking_a_very_low_exchange_read_pushes_nothing() {
    let (pair, service, browser) = subscribed().await;
    let id = sent(&pair, VERY_LOW).await;
    mark_read(&pair, &[id]).await;
    assert_eq!(pushed(&pair, &service, &browser).await, Vec::<Value>::new());
}

#[tokio::test]
async fn a_single_read_racing_many_clears_each_notification_once() {
    let (pair, service, browser) = subscribed().await;
    let ids = notifications(&pair, 2).await;
    let single = format!("/exchanges/{}/@chris", ids[0]);
    tokio::join!(
        request(&pair.app, "PUT", &single, &pair.chris, &[], ""),
        mark_read(&pair, &ids),
    );
    let pushes = pushed(&pair, &service, &browser).await;
    assert_eq!(cleared(&pushes), ids);
}

const NOTIFICATION: &str = r#"{"summary": "Backup finished"}"#;
const VERY_LOW: &str = r#"{"summary": "Synced", "urgency": "very-low"}"#;
const QUESTION: &str = r#"{"summary": "Deploy?", "choices": ["Ship it"]}"#;

/// The events of the pushes about one exchange that `@chris`'s browser
/// receives, from its arrival until retention deletes it, when `@chris`
/// does `first` to it in between.
async fn until_deleted(body: &str, first: impl AsyncFnOnce(&Pair, &str)) -> Vec<String> {
    let (pair, service, browser) = subscribed().await;
    let id = sent(&pair, body).await;
    first(&pair, &id).await;
    let later = now() + pair.app.app.retention.milliseconds() + 60_000;
    retention::delete_due(&pair.app.app, later).await.unwrap();
    pushed(&pair, &service, &browser)
        .await
        .iter()
        .filter(|push| push["id"] == id.as_str())
        .map(|push| push["event"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn retention_closes_a_notification_only_where_it_may_still_show() {
    let events = [
        until_deleted(NOTIFICATION, async |_, _| {}).await,
        until_deleted(VERY_LOW, async |_, _| {}).await,
        until_deleted(NOTIFICATION, async |pair, id| {
            let path = format!("/exchanges/{id}/@chris");
            request(&pair.app, "PUT", &path, &pair.chris, &[], "").await;
        })
        .await,
        until_deleted(QUESTION, async |pair, id| {
            let path = format!("/exchanges/{id}/reply");
            let choice = r#"{"choice": "Ship it"}"#;
            request(&pair.app, "PUT", &path, &pair.chris, &[JSON], choice).await;
        })
        .await,
    ];
    assert_eq!(
        events,
        [
            vec!["received", "gone"],
            vec![],
            vec!["received", "read"],
            vec!["received", "ended"],
        ]
    );
}
