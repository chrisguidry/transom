//! The live feed: `/@{name}/exchanges` as an event stream.

use serde_json::Value;

use super::{decide, household, individual, pair, question, send};
use crate::app::testing::{TestApp, events, request};

/// Gives a feed opened alongside time to subscribe.
async fn opened() {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

/// Each event's name and its message's body.
fn bodies(seen: &[(String, Value)]) -> Vec<(&str, Option<&str>)> {
    seen.iter()
        .map(|(name, data)| (name.as_str(), data["message"]["body"].as_str()))
        .collect()
}

/// `@household` with `@chris` and `@spouse` both members.
async fn two_members() -> (TestApp, String, String) {
    let (app, chris, spouse) = household().await;
    request(&app, "PUT", "/@household/members/@spouse", &chris, &[], "").await;
    decide(&app, &spouse, "spouse", "join").await;
    (app, chris, spouse)
}

#[test]
fn an_exchange_id_is_never_the_groups_notice() {
    use crate::exchanges::changes::Notice;
    let id = crate::exchanges::records::new_id();
    assert_eq!(Notice::parse(id.clone()), Notice::Exchange(id));
    assert_eq!(Notice::parse("groups".to_owned()), Notice::Groups);
}

#[tokio::test]
async fn the_live_feed_announces_new_and_changed_exchanges() {
    let pair = pair().await;
    let stranger = individual(&pair.app, "stranger").await;
    let happening = async {
        opened().await;
        let id = question(&pair).await;
        let mine = format!("/exchanges/{id}/@chris");
        request(&pair.app, "PUT", &mine, &pair.recipient, &[], "").await;
        request(&pair.app, "PUT", &mine, &pair.recipient, &[], "").await;
        request(&pair.app, "DELETE", &mine, &pair.recipient, &[], "").await;
        drop(stranger);
    };
    let seen = events(
        &pair.app,
        "/@chris/exchanges",
        &pair.recipient,
        4,
        happening,
    )
    .await;
    let reads: Vec<_> = seen
        .iter()
        .map(|(_, exchange)| exchange["read"].as_bool())
        .collect();
    assert_eq!(reads, [Some(false), Some(true), Some(false)]);
}

#[tokio::test]
async fn the_senders_feed_never_hears_a_read() {
    let pair = pair().await;
    let happening = async {
        opened().await;
        let id = question(&pair).await;
        let mine = format!("/exchanges/{id}/@chris");
        request(&pair.app, "PUT", &mine, &pair.recipient, &[], "").await;
        send(&pair.app, &pair.sender, "/@chris", &[], "and another").await;
    };
    let seen = events(
        &pair.app,
        "/@review-bot/exchanges",
        &pair.sender,
        2,
        happening,
    )
    .await;
    assert_eq!(
        bodies(&seen),
        [("exchange", None), ("exchange", Some("and another"))]
    );
}

#[tokio::test]
async fn the_senders_feed_carries_no_read_state() {
    let pair = pair().await;
    let happening = async {
        opened().await;
        question(&pair).await;
    };
    let seen = events(
        &pair.app,
        "/@review-bot/exchanges",
        &pair.sender,
        1,
        happening,
    )
    .await;
    let reads: Vec<_> = seen
        .iter()
        .map(|(_, exchange)| exchange.get("read"))
        .collect();
    assert_eq!(reads, [None]);
}

#[tokio::test]
async fn a_feed_ends_when_its_reader_creates_a_group() {
    let pair = pair().await;
    let happening = async {
        opened().await;
        request(&pair.app, "PUT", "/@household", &pair.recipient, &[], "").await;
        send(&pair.app, &pair.sender, "/@chris", &[], "after").await;
    };
    let seen = events(
        &pair.app,
        "/@chris/exchanges",
        &pair.recipient,
        1,
        happening,
    )
    .await;
    assert_eq!(bodies(&seen), []);
}

#[tokio::test]
async fn a_feed_ends_when_its_reader_joins_a_group() {
    let (app, chris, spouse) = household().await;
    request(&app, "PUT", "/@household/members/@spouse", &chris, &[], "").await;
    let happening = async {
        opened().await;
        decide(&app, &spouse, "spouse", "join").await;
        send(&app, &spouse, "/@spouse", &[], "after").await;
    };
    let seen = events(&app, "/@spouse/exchanges", &spouse, 1, happening).await;
    assert_eq!(bodies(&seen), []);
}

#[tokio::test]
async fn a_feed_ends_after_the_gone_events_of_the_group_its_reader_deleted() {
    let (app, chris, _) = household().await;
    send(&app, &chris, "/@household", &[], "Dinner at 6").await;
    let happening = async {
        opened().await;
        request(
            &app,
            "DELETE",
            "/@household/members/@chris",
            &chris,
            &[],
            "",
        )
        .await;
        send(&app, &chris, "/@chris", &[], "after").await;
    };
    let seen = events(&app, "/@chris/exchanges", &chris, 2, happening).await;
    assert_eq!(bodies(&seen), [("gone", None)]);
}

#[tokio::test]
async fn a_feed_ends_when_its_reader_is_removed_from_a_group() {
    let (app, chris, spouse) = two_members().await;
    let happening = async {
        opened().await;
        request(
            &app,
            "DELETE",
            "/@household/members/@spouse",
            &chris,
            &[],
            "",
        )
        .await;
        send(&app, &spouse, "/@spouse", &[], "after").await;
    };
    let seen = events(&app, "/@spouse/exchanges", &spouse, 1, happening).await;
    assert_eq!(bodies(&seen), []);
}

#[tokio::test]
async fn a_groups_feed_ends_when_its_reader_leaves_the_group() {
    let (app, chris, spouse) = two_members().await;
    let happening = async {
        opened().await;
        request(
            &app,
            "DELETE",
            "/@household/members/@chris",
            &chris,
            &[],
            "",
        )
        .await;
        send(&app, &spouse, "/@household", &[], "after").await;
    };
    let seen = events(&app, "/@household/exchanges", &chris, 1, happening).await;
    assert_eq!(bodies(&seen), []);
}

#[tokio::test]
async fn a_groups_feed_sends_only_the_groups_exchanges() {
    let (app, chris, _) = household().await;
    let happening = async {
        opened().await;
        request(&app, "PUT", "/@family", &chris, &[], "").await;
        send(&app, &chris, "/@chris", &[], "note to self").await;
        send(&app, &chris, "/@household", &[], "Taking the trash out").await;
    };
    let seen = events(&app, "/@household/exchanges", &chris, 1, happening).await;
    assert_eq!(bodies(&seen), [("exchange", Some("Taking the trash out"))]);
}
