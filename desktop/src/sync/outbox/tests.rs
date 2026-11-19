//! The outbox's queue, kept in its file, and its delivery to a real
//! server.

use url::Url;

use super::*;
use crate::exchange::State;
use crate::testserver::{NOWHERE, Pair};

fn reply(exchange: &str, choice: &str) -> Queued {
    Queued::Reply {
        exchange: exchange.to_owned(),
        sending: Sending {
            choice: Some(choice.to_owned()),
            text: None,
        },
        origin: Origin::Notification,
    }
}

fn read(exchange: &str, read: bool) -> Queued {
    Queued::Read {
        exchange: exchange.to_owned(),
        read,
    }
}

fn path(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("transom-outbox-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    directory.join("outbox.json")
}

#[test]
fn what_is_queued_is_still_queued_after_a_restart() {
    let path = path("restart");
    let mut outbox = Outbox::open(&path);
    outbox.put(reply("a", "Yes"));
    outbox.put(read("b", true));
    let reopened = Outbox::open(&path);
    assert_eq!(reopened.queued(), [reply("a", "Yes"), read("b", true)]);
    assert_eq!(
        reopened.reply_to("a").unwrap().choice.as_deref(),
        Some("Yes")
    );
    assert_eq!(reopened.marked("b"), Some(true));
}

#[test]
fn the_first_reply_stays_and_the_last_read_mark_wins() {
    let mut outbox = Outbox::open(&path("wins"));
    outbox.put(reply("a", "Yes"));
    outbox.put(reply("a", "No"));
    outbox.put(read("a", true));
    outbox.put(read("a", false));
    assert_eq!(outbox.queued(), [reply("a", "Yes"), read("a", false)]);
}

#[test]
fn a_sent_item_leaves_the_queue_and_clear_empties_it() {
    let path = path("sent");
    let mut outbox = Outbox::open(&path);
    outbox.put(reply("a", "Yes"));
    outbox.put(read("b", true));
    outbox.sent(&reply("a", "Yes"));
    assert_eq!(Outbox::open(&path).queued(), [read("b", true)]);
    outbox.clear();
    assert_eq!(Outbox::open(&path).queued(), []);
}

/// What a run of the outbox settled, and how the run ended.
type Run = (Vec<(Queued, Settled)>, Result<(), Failure>);

/// Sends `items` with `client` as `me`.
async fn delivered(client: &Client, me: &str, items: &[Queued]) -> Run {
    let mut settled = Vec::new();
    let report = |item, outcome| settled.push((item, outcome));
    let ended = deliver(client, me, items.to_vec(), report).await;
    (settled, ended)
}

/// Whether the server has `id` as read by `@bob`.
async fn read_on_server(pair: &Pair, id: &str) -> bool {
    let listed = pair.server.exchanges(&pair.bob).await;
    let exchange = listed.iter().find(|exchange| exchange["id"] == id);
    exchange.unwrap()["read"].as_bool().unwrap()
}

#[tokio::test]
async fn a_reply_the_server_takes_is_accepted() {
    let pair = Pair::start().await;
    let id = pair.ask("Restart?", &["Yes", "No"]).await;
    let items = [reply(&id, "Yes")];
    let (settled, ended) = delivered(&pair.client(), "@bob", &items).await;
    assert!(ended.is_ok());
    assert!(matches!(
        &settled[..],
        [(item, Settled::Accepted(exchange))] if *item == items[0] && exchange.state == State::Replied
    ));
}

#[tokio::test]
async fn a_reply_after_another_is_lost_to_it() {
    let pair = Pair::start().await;
    let id = pair.ask("Restart?", &["Yes", "No"]).await;
    pair.server.reply(&pair.bob, &id, "Yes").await;
    let (settled, ended) = delivered(&pair.client(), "@bob", &[reply(&id, "No")]).await;
    assert!(ended.is_ok());
    assert!(matches!(
        &settled[..],
        [(_, Settled::Lost { winner, .. })] if winner.choice.as_deref() == Some("Yes")
    ));
}

#[tokio::test]
async fn a_reply_the_server_would_refuse_again_is_refused_and_the_rest_is_sent() {
    let pair = Pair::start().await;
    let withdrawn = pair.ask("Restart?", &["Yes", "No"]).await;
    let client = pair.server.client(&pair.alice);
    client.withdraw(&withdrawn).await.unwrap();
    let open = pair.ask("Rotate?", &["Yes", "No"]).await;
    let items = [reply(&withdrawn, "Yes"), reply(&open, "Yes")];
    let (settled, ended) = delivered(&pair.client(), "@bob", &items).await;
    assert!(ended.is_ok());
    assert!(matches!(
        &settled[..],
        [(_, Settled::Refused(reason)), (_, Settled::Accepted(_))]
            if reason == "transom answered 409: the exchange is withdrawn"
    ));
}

#[tokio::test]
async fn an_exchange_is_marked_unread() {
    let pair = Pair::start().await;
    let id = pair.tell("Backup finished").await;
    pair.client().mark(&id, "@bob", true).await.unwrap();
    let (settled, ended) = delivered(&pair.client(), "@bob", &[read(&id, false)]).await;
    assert!(ended.is_ok());
    assert!(matches!(&settled[..], [(_, Settled::Marked { .. })]));
    assert!(!read_on_server(&pair, &id).await);
}

#[tokio::test]
async fn every_exchange_marked_read_is_marked_after_the_replies() {
    let pair = Pair::start().await;
    let (first, second) = (pair.tell("First").await, pair.tell("Second").await);
    let asked = pair.ask("Restart?", &["Yes", "No"]).await;
    let items = [
        read(&first, true),
        reply(&asked, "Yes"),
        read(&second, true),
    ];
    let (settled, ended) = delivered(&pair.client(), "@bob", &items).await;
    assert!(ended.is_ok());
    assert!(matches!(
        &settled[..],
        [(_, Settled::Accepted(_)), (one, Settled::Marked { .. }), (other, Settled::Marked { .. })]
            if *one == items[0] && *other == items[2]
    ));
    assert!(read_on_server(&pair, &first).await);
    assert!(read_on_server(&pair, &second).await);
}

#[tokio::test]
async fn read_marks_the_server_would_refuse_again_are_each_refused() {
    let pair = Pair::start().await;
    let (first, second) = (pair.tell("First").await, pair.tell("Second").await);
    let items = [read(&first, true), read(&second, true)];
    // The server marks an exchange read only for the person who asks.
    let (settled, ended) = delivered(&pair.client(), "@alice", &items).await;
    assert!(ended.is_ok());
    assert!(matches!(
        &settled[..],
        [(_, Settled::Refused(_)), (_, Settled::Refused(_))]
    ));
    assert!(!read_on_server(&pair, &first).await);
}

/// A client of a server that cannot be reached.
fn nowhere() -> Client {
    Client::new(&Url::parse(NOWHERE).unwrap(), "token")
}

#[tokio::test]
async fn a_run_stops_at_a_reply_that_has_to_be_sent_again_later() {
    let items = [reply("a", "Yes"), reply("b", "Yes")];
    let (settled, ended) = delivered(&nowhere(), "@bob", &items).await;
    assert!(settled.is_empty());
    assert!(ended.is_err_and(|failure| failure.transient()));
}

#[tokio::test]
async fn a_run_stops_at_read_marks_that_have_to_be_sent_again_later() {
    let (settled, ended) = delivered(&nowhere(), "@bob", &[read("a", true)]).await;
    assert!(settled.is_empty());
    assert!(ended.is_err_and(|failure| failure.transient()));
}

#[test]
fn an_outbox_that_cannot_be_written_still_queues() {
    let file = path("unwritable");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "not a directory").unwrap();
    let mut outbox = Outbox::open(&file.join("outbox.json"));
    outbox.put(reply("a", "Yes"));
    assert_eq!(outbox.queued(), [reply("a", "Yes")]);
}
