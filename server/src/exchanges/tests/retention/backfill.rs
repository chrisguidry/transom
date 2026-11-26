//! The backfill: exchanges that ended or arrived before retention existed
//! have no deletion scheduled, and the first pass schedules them.

use super::{after_expiry_kept, after_retention, me};
use crate::app::testing::{TestApp, request};
use crate::exchanges::records::now;
use crate::exchanges::tests::{household, individual, send};
use crate::exchanges::{expiry, retention};

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
    let expiring = me.delete_due(after_retention(&me.app)).await;
    let deleted = me.delete_due(after_expiry_kept(&me.app)).await;
    assert_eq!((expiring + deleted, me.listed().await), (2, vec![open]));
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
    me.delete_due(after_retention(&me.app)).await;
    assert_eq!(me.delete_due(after_expiry_kept(&me.app)).await, 1);
}
