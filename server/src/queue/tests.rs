//! Claiming items, as several servers sharing one Redis do.

use std::time::Duration;

use super::Queue;
use crate::app::testing::TestApp;

/// The queue the tests use, made when each test runs rather than as a
/// constant, as the servers' queues are, so that the tests run
/// `Queue::new` too.
fn queue() -> Queue {
    Queue::new("tests", "test", Duration::from_secs(60))
}

const LEASE: i64 = 60_000;

const NOW: i64 = 1_000_000;

/// transom with one item in the queue, due at `due`.
async fn queued(due: i64) -> TestApp {
    let app = TestApp::new().await;
    let mut pipe = redis::pipe();
    pipe.atomic();
    queue().add(&mut pipe, &app.app.store, "item", &[("attempts", "0")], due);
    let mut connection = app.app.store.connection().await.unwrap();
    pipe.query_async::<()>(&mut connection).await.unwrap();
    app
}

async fn claim(app: &TestApp, at: i64) -> bool {
    queue().claim(&app.app, "item", at).await.unwrap()
}

#[tokio::test]
async fn a_due_item_is_claimed_by_one_server_at_a_time() {
    let app = queued(NOW).await;
    assert_eq!(
        (claim(&app, NOW).await, claim(&app, NOW).await),
        (true, false)
    );
}

#[tokio::test]
async fn an_item_whose_lease_ran_out_is_claimed_again() {
    let app = queued(NOW).await;
    claim(&app, NOW).await;
    assert!(!claim(&app, NOW + LEASE - 1).await);
    assert!(claim(&app, NOW + LEASE).await);
}

#[tokio::test]
async fn an_item_not_yet_due_is_not_claimed() {
    let app = queued(NOW + 1).await;
    assert!(!claim(&app, NOW).await);
}

#[tokio::test]
async fn a_finished_item_is_gone() {
    let app = queued(NOW).await;
    claim(&app, NOW).await;
    queue().finish(&app.app.store, "item").await.unwrap();
    assert!(!claim(&app, NOW + LEASE).await);
    let fields = queue().fields(&app.app.store, "item").await.unwrap();
    assert_eq!(fields, std::collections::HashMap::new());
}

#[tokio::test]
async fn a_stopping_server_claims_nothing() {
    let app = queued(NOW).await;
    app.app.stopping.cancel();
    assert!(!claim(&app, NOW).await);
}

#[tokio::test]
async fn a_retried_item_comes_due_when_it_was_scheduled() {
    let app = queued(NOW).await;
    let store = &app.app.store;
    claim(&app, NOW).await;
    queue().retry(store, "item", 1, NOW + 5000).await.unwrap();
    let early = queue().due(store, NOW + 4999).await.unwrap();
    assert_eq!(early, Vec::<String>::new());
    assert_eq!(queue().due(store, NOW + 5000).await.unwrap(), ["item"]);
    assert_eq!(
        queue().fields(store, "item").await.unwrap()["attempts"],
        "1"
    );
}
