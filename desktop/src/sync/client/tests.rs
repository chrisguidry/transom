//! The client against a real server: each call the app makes, and each
//! way the server refuses one.

use reqwest::Method;
use serde_json::{Value, json};
use url::Url;

use super::*;
use crate::exchange::State;
use crate::testserver::{NOWHERE, PICTURE, Pair, TestServer};

#[tokio::test]
async fn the_list_has_what_was_sent_unread() {
    let pair = Pair::start().await;
    let id = pair.tell("Backup finished").await;
    let listed = pair.client().exchanges("@bob").await.unwrap();
    assert_eq!(listed[0].id, id);
    assert_eq!(
        listed[0].message.summary.as_deref(),
        Some("Backup finished")
    );
    assert_eq!(listed[0].read, Some(false));
}

#[tokio::test]
async fn a_reply_is_accepted_with_the_exchange_it_ended() {
    let pair = Pair::start().await;
    let id = pair.ask("Restart?", &["Yes", "No"]).await;
    let replied = pair.client().reply(&id, Some("Yes"), None).await;
    assert!(matches!(
        replied,
        Ok(Replied::Accepted(exchange))
            if exchange.state == State::Replied && exchange.reply.as_ref().unwrap().choice.as_deref() == Some("Yes")
    ));
}

#[tokio::test]
async fn a_reply_with_text_is_accepted() {
    let pair = Pair::start().await;
    let question = json!({ "summary": "Why?", "prompt": "Say why" });
    let id = pair.server.send(&pair.alice, "@bob", question).await;
    pair.client()
        .reply(&id, None, Some("Because"))
        .await
        .unwrap();
    let exchange = pair.server.exchange(&pair.bob, &id).await;
    assert_eq!(exchange["reply"]["text"], "Because");
}

#[tokio::test]
async fn a_reply_after_another_loses_to_it() {
    let pair = Pair::start().await;
    let id = pair.ask("Restart?", &["Yes", "No"]).await;
    pair.server.reply(&pair.bob, &id, "Yes").await;
    let replied = pair.client().reply(&id, Some("No"), None).await;
    assert!(matches!(
        replied,
        Ok(Replied::Lost(winner)) if winner.by == "@bob" && winner.choice.as_deref() == Some("Yes")
    ));
}

#[tokio::test]
async fn the_same_reply_again_is_accepted() {
    let pair = Pair::start().await;
    let id = pair.ask("Restart?", &["Yes", "No"]).await;
    pair.server.reply(&pair.bob, &id, "Yes").await;
    let replied = pair.client().reply(&id, Some("Yes"), None).await;
    assert!(matches!(replied, Ok(Replied::Accepted(_))));
}

#[tokio::test]
async fn a_reply_to_a_withdrawn_message_is_refused_for_good() {
    let pair = Pair::start().await;
    let id = pair.ask("Restart?", &["Yes", "No"]).await;
    pair.server.client(&pair.alice).withdraw(&id).await.unwrap();
    let failure = pair
        .client()
        .reply(&id, Some("Yes"), None)
        .await
        .unwrap_err();
    assert_eq!(
        failure.to_string(),
        "transom answered 409: the exchange is withdrawn"
    );
    assert!(!failure.transient());
}

#[tokio::test]
async fn only_the_sender_withdraws_a_message() {
    let pair = Pair::start().await;
    let id = pair.ask("Restart?", &["Yes", "No"]).await;
    let failure = pair.client().withdraw(&id).await.unwrap_err();
    assert_eq!(
        failure.to_string(),
        "transom answered 403: only the sender may withdraw a message"
    );
    let exchange = pair.server.exchange(&pair.bob, &id).await;
    assert_eq!(exchange["state"], "open");
}

/// Whether the server has `id` as read by `@bob`.
async fn read(pair: &Pair, id: &str) -> Value {
    let listed = pair.server.exchanges(&pair.bob).await;
    let exchange = listed.iter().find(|exchange| exchange["id"] == id);
    exchange.unwrap()["read"].clone()
}

#[tokio::test]
async fn an_exchange_is_marked_read_and_unread() {
    let pair = Pair::start().await;
    let id = pair.tell("Backup finished").await;
    pair.client().mark(&id, "@bob", true).await.unwrap();
    assert_eq!(read(&pair, &id).await, true);
    pair.client().mark(&id, "@bob", false).await.unwrap();
    assert_eq!(read(&pair, &id).await, false);
}

#[tokio::test]
async fn several_exchanges_are_marked_read_in_one_request() {
    let pair = Pair::start().await;
    let ids = [pair.tell("First").await, pair.tell("Second").await];
    pair.client().mark_read("@bob", &ids).await.unwrap();
    assert_eq!(read(&pair, &ids[0]).await, true);
    assert_eq!(read(&pair, &ids[1]).await, true);
}

#[tokio::test]
async fn this_computer_is_registered_as_a_device_and_forgotten() {
    let pair = Pair::start().await;
    let client = pair.client();
    client.register("@bob", "desk", "vega").await.unwrap();
    let devices = client.devices("@bob").await.unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(
        (devices[0].id.as_str(), devices[0].name.as_str()),
        ("desk", "vega")
    );
    assert_eq!(devices[0].wakes(), "Direct connection");
    client.forget("@bob", "desk").await.unwrap();
    assert_eq!(client.devices("@bob").await.unwrap(), []);
}

#[test]
fn a_device_says_how_it_hears_of_a_message() {
    let cases = [
        (json!({ "web_push": {} }), "Web Push"),
        (json!({ "fcm": "token" }), "Google Play services"),
        (json!({ "connection": true }), "Direct connection"),
    ];
    for (wake, said) in cases {
        let device: Device =
            serde_json::from_value(json!({ "id": "a", "name": "A", "wake": wake })).unwrap();
        assert_eq!(device.wakes(), said);
    }
}

#[tokio::test]
async fn an_identity_is_described_and_changed() {
    let pair = Pair::start().await;
    let client = pair.client();
    let alice = client.identity("@alice").await.unwrap();
    assert_eq!(
        (alice.name.as_str(), alice.kind),
        ("alice", Kind::Individual)
    );
    let change = json!({ "display_name": "Bob Example" });
    let bob = client.change("@bob", change).await.unwrap();
    assert_eq!(bob.display_name.as_deref(), Some("Bob Example"));
}

#[tokio::test]
async fn a_groups_members_are_listed_invited_and_removed() {
    let pair = Pair::start().await;
    let (client, server) = (pair.client(), &pair.server);
    let carol = server.person("carol").await;
    server
        .call(Method::PUT, "/@team", &pair.bob, Value::Null)
        .await;
    assert_eq!(client.identity("@team").await.unwrap().kind, Kind::Group);
    client.invite("@team", "@carol").await.unwrap();
    server.answer_transom(&carol, "join").await;
    assert_eq!(client.members("@team").await.unwrap(), ["@bob", "@carol"]);
    client.remove("@team", "@carol").await.unwrap();
    assert_eq!(client.members("@team").await.unwrap(), ["@bob"]);
}

#[tokio::test]
async fn a_picture_is_fetched_from_the_sessions_own_server() {
    let pair = Pair::start().await;
    let picture = pair.pictured("Backup finished").await;
    assert_eq!(pair.client().picture(&picture).await.unwrap(), PICTURE);
    let elsewhere = Url::parse("https://elsewhere.example/").unwrap();
    let elsewhere = elsewhere.join(&picture).unwrap();
    let fetched = pair.client().picture(elsewhere.as_str()).await;
    assert_eq!(fetched.unwrap(), PICTURE);
}

#[tokio::test]
async fn an_attached_image_is_fetched() {
    let pair = Pair::start().await;
    let id = pair.attach("graph.png").await;
    let fetched = pair.client().attachment(&id, "graph.png").await;
    assert_eq!(fetched.unwrap(), PICTURE);
}

#[tokio::test]
async fn a_server_that_is_not_there_is_unreachable_for_now() {
    let server = Url::parse(NOWHERE).unwrap();
    let failure = Client::new(&server, "token")
        .exchanges("@bob")
        .await
        .unwrap_err();
    assert!(matches!(failure, Failure::Unreachable(_)));
    assert!(
        failure
            .to_string()
            .starts_with("the server could not be reached: ")
    );
    assert!(failure.transient());
    assert!(!failure.signed_out());
}

#[tokio::test]
async fn a_token_the_server_does_not_take_is_signed_out() {
    let server = TestServer::start().await;
    let client = Client::new(&server.url, "not-a-token");
    let failure = client.exchanges("@bob").await.unwrap_err();
    assert!(matches!(failure, Failure::Refused { status: 401, .. }));
    assert!(failure.signed_out());
    assert!(failure.transient());
}

#[tokio::test]
async fn a_refusal_that_is_not_transoms_is_told_as_it_came() {
    let server = TestServer::start().await;
    let client = Client::new(&server.issuer(), "token");
    let failure = client.identity("@bob").await.unwrap_err();
    assert!(matches!(failure, Failure::Refused { status: 404, reason, .. } if reason.is_empty()));
}

fn refused(status: u16) -> Failure {
    Failure::Refused {
        status,
        reason: String::new(),
        winner: None,
    }
}

#[test]
fn a_refusal_may_succeed_later_when_the_server_or_the_session_is_at_fault() {
    for status in [401, 408, 429, 500, 503] {
        assert!(refused(status).transient(), "{status}");
    }
    for status in [400, 403, 404, 409] {
        assert!(!refused(status).transient(), "{status}");
    }
}
