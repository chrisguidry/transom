//! Retention: an exchange is deleted once the `retention` setting has
//! passed since it ended, or for a notification, since it arrived.

use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use super::{JSON, decide, household, individual, send};
use crate::app::testing::{TestApp, events, request};
use crate::exchanges::records::now;
use crate::exchanges::{expiry, retention};

/// A moment after the retention of anything that ended or arrived by now.
fn after_retention(app: &TestApp) -> i64 {
    now() + app.app.retention.milliseconds() + 1000
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
    let late = me.delete_due(after_retention(&me.app)).await;
    assert_eq!(
        (early, late, me.status(&id).await),
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
    // The consent question ended with the release, and goes with it.
    assert_eq!((while_held.unwrap(), after_release.unwrap()), (0, 2));
    assert_eq!(status_of(&app, &chris, id).await, StatusCode::NOT_FOUND);
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
    let marked = crate::exchanges::records::set_read(&me.app.app.store, "anyone", &id, true).await;
    assert_eq!(
        (marked.unwrap(), mentions(&me.app, &id).await),
        (false, Vec::new())
    );
}

#[tokio::test]
async fn the_live_feed_says_a_deleted_exchange_is_gone() {
    let me = me().await;
    let id = me.notification().await;
    let happening = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        me.delete_due(after_retention(&me.app)).await;
    };
    let seen = events(&me.app, "/@chris/exchanges", &me.token, 1, happening).await;
    assert_eq!(seen, [("gone".to_owned(), json!({ "id": id }))]);
}

#[tokio::test]
async fn two_servers_sweeping_at_once_delete_each_exchange_once() {
    let me = me().await;
    for _ in 0..5 {
        me.notification().await;
    }
    let later = after_retention(&me.app);
    let (first, second) = tokio::join!(me.delete_due(later), me.delete_due(later));
    assert_eq!(first + second, 5);
}

/// Takes every exchange out of the deletion schedule, as data written
/// before retention existed has none.
async fn unschedule(app: &TestApp) {
    let mut connection = app.app.store.connection().await.unwrap();
    redis::cmd("DEL")
        .arg(app.app.store.key("deletions"))
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_backfill_schedules_what_ended_or_arrived_before_retention_existed() {
    let me = me().await;
    let open = me.question().await;
    let replied = me.question().await;
    me.reply(&replied).await;
    me.notification().await;
    unschedule(&me.app).await;
    // A backfill that runs again changes nothing.
    retention::backfill(&me.app.app, now()).await.unwrap();
    retention::backfill(&me.app.app, now() + 3_600_000)
        .await
        .unwrap();
    let deleted = me.delete_due(after_retention(&me.app)).await;
    assert_eq!((deleted, me.listed().await), (2, vec![open]));
}

#[tokio::test]
async fn the_backfill_keeps_a_withdrawn_exchange_a_whole_retention_from_when_it_runs() {
    let me = me().await;
    let id = me.question().await;
    me.withdraw(&id).await;
    unschedule(&me.app).await;
    let started = now() + 3_600_000;
    retention::backfill(&me.app.app, started).await.unwrap();
    let early = me.delete_due(after_retention(&me.app)).await;
    let late = me.delete_due(after_retention(&me.app) + 3_600_000).await;
    assert_eq!((early, late), (0, 1));
}

#[tokio::test]
async fn the_backfill_schedules_an_expired_question_by_its_expiry() {
    let me = me().await;
    me.send(r#"{"summary": "Now?", "choices": ["Yes"], "ttl": 1}"#)
        .await;
    expiry::expire_due(&me.app.app, now() + 2000).await.unwrap();
    unschedule(&me.app).await;
    retention::backfill(&me.app.app, now() + 3_600_000)
        .await
        .unwrap();
    let deleted = me.delete_due(after_retention(&me.app) + 2000).await;
    assert_eq!(deleted, 1);
}

#[tokio::test]
async fn the_backfill_passes_over_a_held_message_and_a_deleted_groups_tombstone() {
    let (app, chris, _) = household().await;
    let stranger = individual(&app, "stranger").await;
    send(&app, &stranger, "/@chris", &[], "Hello").await;
    send(&app, &chris, "/@household", &[], "Dinner at 6").await;
    request(
        &app,
        "DELETE",
        "/@household/members/@chris",
        &chris,
        &[],
        "",
    )
    .await;
    unschedule(&app).await;
    retention::backfill(&app.app, now()).await.unwrap();
    let deleted = retention::delete_due(&app.app, after_retention(&app)).await;
    // What is left is the open consent question, which never ages.
    assert_eq!(deleted.unwrap(), 0);
}

#[tokio::test]
async fn the_first_pass_runs_the_backfill() {
    let me = me().await;
    me.notification().await;
    unschedule(&me.app).await;
    expiry::tick(&me.app.app).await;
    assert_eq!(me.delete_due(after_retention(&me.app)).await, 1);
}

#[tokio::test]
async fn a_pass_deletes_what_is_due() {
    let app = TestApp::with_retention(std::time::Duration::ZERO).await;
    let chris = individual(&app, "chris").await;
    send(&app, &chris, "/@chris", &[], "a note to myself").await;
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
