//! Following the feed of a real server: what the app hears as the
//! server's list changes, as the connection is lost and found, and as the
//! session ends.

use std::time::{Duration, Instant};

use reqwest::Method;
use serde_json::Value;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::task::JoinHandle;
use url::Url;

use super::*;
use crate::testserver::{NOWHERE, Pair, TestServer, canned};

/// A feed being followed: what it hears, the way to say the computer
/// woke, and the task that follows it.
struct Following {
    heard: UnboundedReceiver<Heard>,
    woke: Arc<Notify>,
    task: JoinHandle<()>,
}

impl Following {
    /// The next thing heard. It fails the test when nothing is heard in
    /// ten seconds.
    async fn next(&mut self) -> Heard {
        let next = tokio::time::timeout(Duration::from_secs(10), self.heard.recv());
        next.await.expect("the feed says something").unwrap()
    }

    /// The ids of the next list heard, after anything heard before it.
    async fn listed(&mut self) -> Vec<String> {
        loop {
            if let Heard::Listed(listed) = self.next().await {
                return listed.into_iter().map(|exchange| exchange.id).collect();
            }
        }
    }
}

/// Follows `me`'s feed with `client`, as the device `desk` of session
/// 1.
fn follow_with(client: Client, me: &str) -> Following {
    let (sender, heard) = unbounded_channel();
    let woke = Arc::new(Notify::new());
    let following = follow(
        client.reporting(1, sender.clone()),
        me.to_owned(),
        "desk".to_owned(),
        sender,
        woke.clone(),
    );
    Following {
        heard,
        woke,
        task: tokio::spawn(following),
    }
}

/// Follows `@bob`'s feed on `pair`'s server.
fn following(pair: &Pair) -> Following {
    follow_with(pair.client(), "@bob")
}

#[tokio::test]
async fn the_whole_list_is_heard_when_the_feed_opens() {
    let pair = Pair::start().await;
    let id = pair.tell("Backup finished").await;
    let mut following = following(&pair);
    assert_eq!(following.listed().await[0], id);
}

#[tokio::test]
async fn a_change_is_heard_after_the_list() {
    let pair = Pair::start().await;
    let mut following = following(&pair);
    following.listed().await;
    let id = pair.tell("Backup finished").await;
    assert!(matches!(following.next().await, Heard::Changed(exchange) if exchange.id == id));
}

#[tokio::test]
async fn this_computer_is_registered_as_a_device_once_the_list_is_heard() {
    let pair = Pair::start().await;
    let mut following = following(&pair);
    following.listed().await;
    // The feed sends what changes only after the device is registered.
    pair.tell("Backup finished").await;
    following.next().await;
    let devices = pair.client().devices("@bob").await.unwrap();
    assert_eq!(devices[0].id, "desk");
    assert_eq!(devices[0].name, hostname());
}

#[tokio::test]
async fn an_exchange_the_server_deleted_is_heard_as_gone() {
    let pair = Pair::start().await;
    let (server, bob) = (&pair.server, &pair.bob);
    server.call(Method::PUT, "/@team", bob, Value::Null).await;
    let id = pair
        .server
        .send(bob, "@team", serde_json::json!({ "summary": "Lunch" }))
        .await;
    let mut following = following(&pair);
    following.listed().await;
    // A group is deleted, with what it received, when its last member
    // leaves.
    pair.client().remove("@team", "@bob").await.unwrap();
    assert!(matches!(following.next().await, Heard::Gone(gone) if gone == id));
}

#[tokio::test]
async fn a_feed_the_server_closes_is_followed_again_after_the_backoff() {
    let pair = Pair::start().await;
    let mut following = following(&pair);
    following.listed().await;
    // The server ends a feed whose reader joined a group, so the next
    // feed hears the group's exchanges too.
    let team = pair
        .server
        .call(Method::PUT, "/@team", &pair.bob, Value::Null);
    team.await;
    assert!(matches!(following.next().await, Heard::Disconnected));
    let closed = Instant::now();
    following.listed().await;
    assert!(closed.elapsed() >= Duration::from_millis(900));
}

#[tokio::test]
async fn a_wake_while_connected_connects_again_at_once() {
    let pair = Pair::start().await;
    let mut following = following(&pair);
    following.listed().await;
    following.woke.notify_one();
    assert!(matches!(following.next().await, Heard::Disconnected));
    let woke = Instant::now();
    following.listed().await;
    assert!(woke.elapsed() < backoff(0));
}

#[tokio::test]
async fn a_server_that_went_away_is_followed_again_when_it_is_back() {
    let mut pair = Pair::start().await;
    let mut following = following(&pair);
    following.listed().await;
    pair.server.stop();
    assert!(matches!(following.next().await, Heard::Disconnected));
    let id = {
        pair.server.restart().await;
        pair.tell("Backup finished").await
    };
    // The network coming back is what tells the app to try again.
    following.woke.notify_one();
    assert_eq!(following.listed().await[0], id);
}

#[tokio::test]
async fn a_server_that_cannot_be_reached_is_heard_as_disconnected() {
    let server = Url::parse(NOWHERE).unwrap();
    let mut following = follow_with(Client::new(&server, "token"), "@bob");
    assert!(matches!(following.next().await, Heard::Disconnected));
}

#[tokio::test]
async fn a_token_the_server_does_not_take_ends_the_following() {
    let server = TestServer::start().await;
    let client = Client::new(&server.url, "not-a-token");
    let mut following = follow_with(client, "@bob");
    let heard = following.next().await;
    assert!(matches!(heard, Heard::SignedOut { generation: 1, .. }));
    following.task.await.unwrap();
}

#[tokio::test]
async fn a_401_from_before_transom_is_followed_again() {
    let url = canned::answering("401 Unauthorized", &[], "").await;
    let mut following = follow_with(Client::new(&url, "token"), "@bob");
    assert!(matches!(following.next().await, Heard::Disconnected));
    following.woke.notify_one();
    assert!(matches!(following.next().await, Heard::Disconnected));
}

/// Waits for `task` to finish. It fails the test when the task is still
/// running in ten seconds.
async fn finished(task: JoinHandle<()>) {
    let finished = tokio::time::timeout(Duration::from_secs(10), task);
    finished.await.expect("the task finishes").unwrap();
}

#[tokio::test]
async fn the_following_stops_when_no_one_hears_the_list() {
    let pair = Pair::start().await;
    let Following { heard, task, .. } = following(&pair);
    drop(heard);
    finished(task).await;
}

#[tokio::test]
async fn the_following_stops_when_no_one_hears_a_change() {
    let pair = Pair::start().await;
    let mut following = following(&pair);
    following.listed().await;
    drop(following.heard);
    pair.tell("Backup finished").await;
    finished(following.task).await;
}
