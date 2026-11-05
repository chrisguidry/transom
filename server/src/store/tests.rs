//! The store through a failover: its connections drop, its address
//! refuses connections or goes silent for a while, and then the new
//! primary answers.

use std::time::Duration;

use redis::AsyncCommands;

use super::failover::Failover;
use super::subscription::Subscription;
use super::testing::{prefixed, store_at};
use super::{Heard, Store};

/// A store that reaches the test Redis through a failover proxy, and one
/// with the same prefix that reaches it directly, as another server does.
async fn through_failover() -> (Failover, Store, Store) {
    let failover = Failover::start().await;
    let store = store_at(&failover.url);
    let direct = prefixed(
        &super::testing::redis_url(),
        store.key("").trim_end_matches(':'),
    );
    (failover, store, direct)
}

async fn set_and_get(store: &Store, value: &str) -> String {
    let mut connection = store.connection().await.unwrap();
    let () = connection.set(store.key("value"), value).await.unwrap();
    connection.get(store.key("value")).await.unwrap()
}

#[tokio::test]
async fn a_command_after_the_connection_drops_succeeds() {
    let (failover, store, _) = through_failover().await;
    set_and_get(&store, "before").await;
    failover.cut();
    assert_eq!(set_and_get(&store, "after").await, "after");
}

#[tokio::test]
async fn a_transaction_after_the_connection_drops_succeeds() {
    let (failover, store, _) = through_failover().await;
    set_and_get(&store, "before").await;
    failover.cut();
    let mut connection = store.connection().await.unwrap();
    let (value,): (String,) = redis::pipe()
        .atomic()
        .set(store.key("value"), "after")
        .ignore()
        .get(store.key("value"))
        .query_async(&mut connection)
        .await
        .unwrap();
    assert_eq!(value, "after");
}

#[tokio::test]
async fn the_connection_names_the_database_of_the_url() {
    let store = store_at(&super::testing::redis_url());
    let connection = store.connection().await.unwrap();
    assert_eq!(redis::aio::ConnectionLike::get_db(&connection), 0);
}

#[tokio::test]
async fn a_command_waits_out_a_short_failover() {
    let (failover, store, _) = through_failover().await;
    set_and_get(&store, "before").await;
    failover.refuse();
    let recovering = async {
        tokio::time::sleep(Duration::from_secs(3)).await;
        failover.recover();
    };
    let (value, ()) = tokio::join!(set_and_get(&store, "after"), recovering);
    assert_eq!(value, "after");
}

async fn publish(store: &Store, payload: &str) {
    let mut connection = store.connection().await.unwrap();
    let () = connection
        .publish(store.key("channel"), payload)
        .await
        .unwrap();
}

/// What `subscription` hears next, which a test expects within ten
/// seconds.
async fn heard(subscription: &mut Subscription) -> Heard {
    tokio::time::timeout(Duration::from_secs(10), subscription.next())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_subscription_hears_what_is_published() {
    let (_failover, store, direct) = through_failover().await;
    let mut subscription = store.subscribe(&["channel".to_owned()]).await.unwrap();
    publish(&direct, "hello").await;
    assert_eq!(
        heard(&mut subscription).await,
        Heard::Message("hello".to_owned())
    );
}

/// A subscription through `failover` that checks its connection every
/// 100 milliseconds.
async fn pinging(failover: &Failover, store: &Store) -> Subscription {
    Subscription::pinging(
        redis::Client::open(failover.url.as_str()).unwrap(),
        vec![store.key("channel")],
        Duration::from_millis(100),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn a_quiet_subscription_that_still_answers_goes_on() {
    let (failover, store, direct) = through_failover().await;
    let mut subscription = pinging(&failover, &store).await;
    let publishing = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        publish(&direct, "later").await;
    };
    let (heard, ()) = tokio::join!(heard(&mut subscription), publishing);
    assert_eq!(heard, Heard::Message("later".to_owned()));
}

/// What a subscription hears after `outage`, and recovery `seconds` later,
/// and then after a message is published.
async fn after(outage: fn(&Failover), seconds: u64) -> (Heard, Heard) {
    let (failover, store, direct) = through_failover().await;
    let mut subscription = pinging(&failover, &store).await;
    outage(&failover);
    let recovering = async {
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        failover.recover();
    };
    let (resumed, ()) = tokio::join!(heard(&mut subscription), recovering);
    publish(&direct, "again").await;
    (resumed, heard(&mut subscription).await)
}

#[tokio::test]
async fn a_subscription_resumes_after_its_connection_drops() {
    assert_eq!(
        after(Failover::cut, 1).await,
        (Heard::Resumed, Heard::Message("again".to_owned()))
    );
}

#[tokio::test]
async fn a_subscription_resumes_after_redis_refuses_it_for_a_while() {
    assert_eq!(
        after(Failover::refuse, 2).await,
        (Heard::Resumed, Heard::Message("again".to_owned()))
    );
}

#[tokio::test]
async fn a_subscription_resumes_after_its_connection_goes_silent() {
    // Long enough for the ping, and then a subscription on a new
    // connection, to give up waiting before Redis answers again.
    assert_eq!(
        after(Failover::go_silent, 5).await,
        (Heard::Resumed, Heard::Message("again".to_owned()))
    );
}
