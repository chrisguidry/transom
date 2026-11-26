//! Retention: an exchange is deleted once the `retention` setting has
//! passed since it ended, or for a notification, since it arrived.

use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use redis::AsyncCommands;
use serde_json::{Value, json};

use super::{JSON, decide, household, individual, send};
use crate::app::testing::{TestApp, events, request};
use crate::exchanges::records::now;
use crate::exchanges::{expiry, retention};

mod backfill;

/// A moment after the retention of anything that ended or arrived by now.
fn after_retention(app: &TestApp) -> i64 {
    now() + app.app.retention.milliseconds() + 1000
}

/// A moment after an open notification that retention expired by
/// [`after_retention`] is kept.
fn after_expiry_kept(app: &TestApp) -> i64 {
    after_retention(app) + app.app.retention.expired_kept()
}

/// A moment before the retention of anything that ended or arrived by now
/// has passed.
fn within_retention(app: &TestApp) -> i64 {
    now() + app.app.retention.milliseconds() - 60_000
}

/// `@chris`, alone, who sends to himself, so the only exchanges are the
/// ones a test makes.
struct Me {
    app: TestApp,
    token: String,
}

async fn me() -> Me {
    let app = TestApp::new().await;
    let token = individual(&app, "chris").await;
    Me { app, token }
}

impl Me {
    async fn send(&self, body: &str) -> String {
        let (status, exchange) = send(&self.app, &self.token, "/@chris", &[JSON], body).await;
        assert_eq!(status, StatusCode::CREATED);
        exchange["id"].as_str().unwrap().to_owned()
    }

    async fn notification(&self) -> String {
        self.send(r#"{"summary": "Backup finished"}"#).await
    }

    async fn question(&self) -> String {
        self.send(r#"{"summary": "Restart?", "choices": ["Restart", "Leave it"]}"#)
            .await
    }

    async fn reply(&self, id: &str) {
        let path = format!("/exchanges/{id}/reply");
        let (status, _, _) = request(&self.app, "PUT", &path, &self.token, &[], "ok").await;
        assert_eq!(status, StatusCode::OK);
    }

    async fn withdraw(&self, id: &str) {
        let path = format!("/exchanges/{id}/message");
        let (status, _, _) = request(&self.app, "DELETE", &path, &self.token, &[], "").await;
        assert_eq!(status, StatusCode::OK);
    }

    async fn status(&self, id: &str) -> StatusCode {
        status_of(&self.app, &self.token, id).await
    }

    /// The ids `@chris` lists.
    async fn listed(&self) -> Vec<String> {
        let (_, _, text) =
            request(&self.app, "GET", "/@chris/exchanges", &self.token, &[], "").await;
        let list: Value = serde_json::from_str(&text).unwrap();
        list["exchanges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|exchange| exchange["id"].as_str().unwrap().to_owned())
            .collect()
    }

    async fn delete_due(&self, now: i64) -> usize {
        retention::delete_due(&self.app.app, now).await.unwrap()
    }
}

async fn status_of(app: &TestApp, token: &str, id: &str) -> StatusCode {
    let (status, _, _) = request(app, "GET", &format!("/exchanges/{id}"), token, &[], "").await;
    status
}

#[tokio::test]
async fn a_replied_question_is_deleted_once_retention_passes_after_its_reply() {
    let me = me().await;
    let id = me.question().await;
    me.reply(&id).await;
    let early = me.delete_due(within_retention(&me.app)).await;
    let late = me.delete_due(after_retention(&me.app)).await;
    assert_eq!(
        (early, late, me.status(&id).await),
        (0, 1, StatusCode::NOT_FOUND)
    );
}

#[tokio::test]
async fn an_open_question_is_never_deleted_for_its_age() {
    let me = me().await;
    let id = me.question().await;
    let far = now() + 10 * 365 * 24 * 60 * 60 * 1000;
    assert_eq!(
        (me.delete_due(far).await, me.status(&id).await),
        (0, StatusCode::OK)
    );
}

#[tokio::test]
async fn a_notification_is_deleted_once_retention_passes_after_it_arrives() {
    let me = me().await;
    let id = me.notification().await;
    let early = me.delete_due(within_retention(&me.app)).await;
    let expiring = me.delete_due(after_retention(&me.app)).await;
    let late = me.delete_due(after_expiry_kept(&me.app)).await;
    assert_eq!(
        (early, expiring, late, me.status(&id).await),
        (0, 0, 1, StatusCode::NOT_FOUND)
    );
}

#[tokio::test]
async fn an_open_notification_expires_when_its_deletion_is_due_and_is_deleted_a_minute_later() {
    let me = me().await;
    let id = me.notification().await;
    let first = me.delete_due(after_retention(&me.app)).await;
    let (_, _, text) = request(
        &me.app,
        "GET",
        &format!("/exchanges/{id}"),
        &me.token,
        &[],
        "",
    )
    .await;
    let state = serde_json::from_str::<Value>(&text).unwrap()["state"].clone();
    let early = me.delete_due(after_retention(&me.app) + 1000).await;
    let late = me.delete_due(after_expiry_kept(&me.app)).await;
    assert_eq!(
        (first, state, early, late, me.status(&id).await),
        (0, json!("expired"), 0, 1, StatusCode::NOT_FOUND)
    );
}

#[tokio::test]
async fn an_expired_notification_is_kept_no_longer_than_the_retention() {
    let app = TestApp::with_retention(std::time::Duration::from_secs(2)).await;
    let chris = individual(&app, "chris").await;
    let (_, note) = send(&app, &chris, "/@chris", &[], "a note to myself").await;
    let due = now() + 3000;
    let expiring = retention::delete_due(&app.app, due).await.unwrap();
    let deleted = retention::delete_due(&app.app, due + 3000).await.unwrap();
    let id = note["id"].as_str().unwrap();
    assert_eq!(
        (expiring, deleted, status_of(&app, &chris, id).await),
        (0, 1, StatusCode::NOT_FOUND)
    );
}

#[tokio::test]
async fn a_held_notification_ages_from_when_it_is_released() {
    let app = TestApp::new().await;
    let stranger = individual(&app, "stranger").await;
    let chris = individual(&app, "chris").await;
    let (_, held) = send(&app, &stranger, "/@chris", &[], "Hello").await;
    let while_held = retention::delete_due(&app.app, after_retention(&app)).await;
    decide(&app, &chris, "chris", "allow").await;
    let id = held["id"].as_str().unwrap();
    let after_release = retention::delete_due(&app.app, after_retention(&app)).await;
    let kept = retention::delete_due(&app.app, after_expiry_kept(&app)).await;
    // The consent question ended with the release, and goes in the first
    // pass after it; the notification expires there and goes a minute
    // later.
    assert_eq!(
        (while_held.unwrap(), after_release.unwrap(), kept.unwrap()),
        (0, 1, 1)
    );
    assert_eq!(status_of(&app, &chris, id).await, StatusCode::NOT_FOUND);
}

/// The event and state of each webhook queued for `url`, in the order
/// they were queued.
async fn hooked(app: &TestApp, url: &str) -> Vec<(String, String)> {
    let store = &app.app.store;
    let mut connection = store.connection().await.unwrap();
    let mut ids: Vec<String> = connection
        .zrange(store.key("deliveries"), 0, -1)
        .await
        .unwrap();
    // Delivery ids are ULIDs, which sort in the order they were made.
    ids.sort();
    let mut found = Vec::new();
    for id in ids {
        let (to, body): (String, String) = connection
            .hmget(store.key(&format!("delivery:{id}")), &["url", "body"])
            .await
            .unwrap();
        let body: Value = serde_json::from_str(&body).unwrap();
        if to == url {
            found.push((
                body["event"].as_str().unwrap().to_owned(),
                body["exchange"]["state"].as_str().unwrap().to_owned(),
            ));
        }
    }
    found
}

#[tokio::test]
async fn an_open_notification_expires_when_it_is_deleted_and_its_sender_hears_once() {
    let me = me().await;
    let webhook = json!({ "webhook": "https://example.com/hook" }).to_string();
    request(&me.app, "PATCH", "/@chris", &me.token, &[JSON], webhook).await;
    let week = 7 * 24 * 60 * 60;
    let id = me
        .send(&json!({ "summary": "Undo?", "ttl": week }).to_string())
        .await;
    let expiring = me.delete_due(after_retention(&me.app)).await;
    let deleted = me.delete_due(after_expiry_kept(&me.app)).await;
    let past_ttl = now() + (week + 60) * 1000;
    let expired = expiry::expire_due(&me.app.app, past_ttl).await.unwrap();
    let again = me.delete_due(past_ttl).await;
    assert_eq!(
        (expiring, deleted, expired, again, me.status(&id).await),
        (0, 1, 0, 0, StatusCode::NOT_FOUND)
    );
    assert_eq!(
        hooked(&me.app, "https://example.com/hook").await,
        [
            ("received".to_owned(), "open".to_owned()),
            ("ended".to_owned(), "expired".to_owned())
        ]
    );
}

#[tokio::test]
async fn a_notification_that_ended_before_its_deletion_ends_once() {
    let me = me().await;
    let webhook = json!({ "webhook": "https://example.com/hook" }).to_string();
    request(&me.app, "PATCH", "/@chris", &me.token, &[JSON], webhook).await;
    let id = me.notification().await;
    me.withdraw(&id).await;
    me.delete_due(after_retention(&me.app)).await;
    assert_eq!(
        hooked(&me.app, "https://example.com/hook").await,
        [
            ("received".to_owned(), "open".to_owned()),
            ("ended".to_owned(), "withdrawn".to_owned())
        ]
    );
}

/// The keys under the app's prefix that name `id`: in their own name, or as
/// a member, field, or value. A message's text that mentions it is not an
/// index of it, so only whole items count.
async fn mentions(app: &TestApp, id: &str) -> Vec<String> {
    let mut connection = app.app.store.connection().await.unwrap();
    let pattern = format!("{}*", app.app.store.key(""));
    let keys: Vec<String> = redis::cmd("KEYS")
        .arg(pattern)
        .query_async(&mut connection)
        .await
        .unwrap();
    let mut found = Vec::new();
    for key in keys {
        let kind: String = redis::cmd("TYPE")
            .arg(&key)
            .query_async(&mut connection)
            .await
            .unwrap();
        let read = match kind.as_str() {
            "zset" => redis::cmd("ZRANGE").arg(&key).arg(0).arg(-1).clone(),
            "set" => redis::cmd("SMEMBERS").arg(&key).clone(),
            "list" => redis::cmd("LRANGE").arg(&key).arg(0).arg(-1).clone(),
            "hash" => redis::cmd("HGETALL").arg(&key).clone(),
            "string" => redis::cmd("MGET").arg(&key).clone(),
            _ => redis::cmd("TYPE").arg(&key).clone(),
        };
        let held: Vec<String> = read.query_async(&mut connection).await.unwrap_or_default();
        if key.contains(id) || held.iter().any(|item| item == id) {
            found.push(key);
        }
    }
    found
}

#[tokio::test]
async fn deleting_an_exchange_leaves_nothing_that_names_it() {
    let (app, chris, spouse) = household().await;
    request(&app, "PUT", "/@household/members/@spouse", &chris, &[], "").await;
    decide(&app, &spouse, "spouse", "join").await;
    let message = json!({
        "summary": "Look",
        "choices": ["Nice"],
        "ttl": 3600,
        "key": "look",
        "urgency": "urgent",
        "attachments": [{ "name": "a.png", "data": STANDARD.encode(b"\x89PNG\r\n\x1a\n") }],
    });
    let (_, exchange) = send(&app, &chris, "/@household", &[JSON], &message.to_string()).await;
    let id = exchange["id"].as_str().unwrap();
    request(
        &app,
        "PUT",
        &format!("/exchanges/{id}/@spouse"),
        &spouse,
        &[],
        "",
    )
    .await;
    request(
        &app,
        "DELETE",
        "/@household/members/@spouse",
        &spouse,
        &[],
        "",
    )
    .await;
    let path = format!("/exchanges/{id}/reply");
    request(&app, "PUT", &path, &chris, &[], "lovely").await;
    let before = mentions(&app, id).await.len();
    retention::delete_due(&app.app, after_retention(&app))
        .await
        .unwrap();
    assert_eq!((before > 0, mentions(&app, id).await), (true, Vec::new()));
}

#[tokio::test]
async fn a_withdrawn_held_message_leaves_its_hold_when_deleted() {
    let app = TestApp::new().await;
    let stranger = individual(&app, "stranger").await;
    individual(&app, "chris").await;
    let (_, held) = send(&app, &stranger, "/@chris", &[], "Hello").await;
    let id = held["id"].as_str().unwrap();
    let path = format!("/exchanges/{id}/message");
    request(&app, "DELETE", &path, &stranger, &[], "").await;
    retention::delete_due(&app.app, after_retention(&app))
        .await
        .unwrap();
    assert_eq!(mentions(&app, id).await, Vec::<String>::new());
}

#[tokio::test]
async fn a_read_mark_after_the_deletion_leaves_nothing_behind() {
    let me = me().await;
    let id = me.notification().await;
    me.delete_due(after_retention(&me.app)).await;
    me.delete_due(after_expiry_kept(&me.app)).await;
    let marked = crate::exchanges::records::set_read(&me.app.app.store, "anyone", &id, true).await;
    assert_eq!(
        (marked.unwrap(), mentions(&me.app, &id).await),
        (false, Vec::new())
    );
}

#[tokio::test]
async fn the_live_feed_says_a_deleted_exchange_is_gone() {
    let me = me().await;
    let id = me.question().await;
    me.reply(&id).await;
    let happening = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        me.delete_due(after_retention(&me.app)).await;
    };
    let seen = events(&me.app, "/@chris/exchanges", &me.token, 1, happening).await;
    assert_eq!(seen, [("gone".to_owned(), json!({ "id": id }))]);
}

#[tokio::test]
async fn the_live_feed_says_a_deleted_open_notification_expired_then_is_gone() {
    let me = me().await;
    let id = me.notification().await;
    // Each feed reads both of its events before the next pass runs, so the
    // deletion cannot race the reads of the expired notification. `@chris`
    // is both the sender and the recipient, so a feed hears each change
    // once for each.
    let expiring = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        me.delete_due(after_retention(&me.app)).await;
    };
    let mut seen = events(&me.app, "/@chris/exchanges", &me.token, 2, expiring).await;
    let deleting = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        me.delete_due(after_expiry_kept(&me.app)).await;
    };
    seen.extend(events(&me.app, "/@chris/exchanges", &me.token, 2, deleting).await);
    let seen: Vec<_> = seen
        .iter()
        .map(|(event, data)| (event.as_str(), data["state"].clone(), data["id"].clone()))
        .collect();
    assert_eq!(
        seen,
        [
            ("exchange", json!("expired"), json!(id)),
            ("exchange", json!("expired"), json!(id)),
            ("gone", Value::Null, json!(id)),
            ("gone", Value::Null, json!(id))
        ]
    );
}

#[tokio::test]
async fn two_servers_sweeping_at_once_delete_each_exchange_once() {
    let me = me().await;
    for _ in 0..5 {
        me.notification().await;
    }
    let later = after_retention(&me.app);
    let expiring = tokio::join!(me.delete_due(later), me.delete_due(later));
    let kept = after_expiry_kept(&me.app);
    let (first, second) = tokio::join!(me.delete_due(kept), me.delete_due(kept));
    assert_eq!((expiring, first + second), ((0, 0), 5));
}

#[tokio::test]
async fn a_pass_deletes_what_is_due() {
    let app = TestApp::with_retention(std::time::Duration::ZERO).await;
    let chris = individual(&app, "chris").await;
    let (_, note) = send(&app, &chris, "/@chris", &[], "a note to myself").await;
    let path = format!("/exchanges/{}/message", note["id"].as_str().unwrap());
    request(&app, "DELETE", &path, &chris, &[], "").await;
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    expiry::tick(&app.app).await;
    let (_, _, text) = request(&app, "GET", "/@chris/exchanges", &chris, &[], "").await;
    assert_eq!(text, r#"{"exchanges":[]}"#);
}

#[tokio::test]
async fn a_deleted_groups_tombstone_that_was_scheduled_is_deleted_too() {
    let (app, chris, _) = household().await;
    let (_, exchange) = send(&app, &chris, "/@household", &[], "Dinner at 6").await;
    let id = exchange["id"].as_str().unwrap();
    request(
        &app,
        "DELETE",
        "/@household/members/@chris",
        &chris,
        &[],
        "",
    )
    .await;
    let gone = status_of(&app, &chris, id).await;
    retention::delete_due(&app.app, after_retention(&app))
        .await
        .unwrap();
    assert_eq!(
        (gone, status_of(&app, &chris, id).await),
        (StatusCode::GONE, StatusCode::NOT_FOUND)
    );
}
