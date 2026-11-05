//! Replacing: a message with a key replaces the open exchange with the
//! same key from the same sender to the same recipient.

use axum::http::StatusCode;
use serde_json::{Value, json};

use super::{JSON, Pair, decide, individual, pair, send};
use crate::app::testing::request;

/// Sends `@chris` a message from `token` with `key`, and returns it.
async fn keyed(pair: &Pair, token: &str, key: &str) -> Value {
    let body = json!({ "summary": "Disk is filling", "key": key }).to_string();
    let (status, exchange) = send(&pair.app, token, "/@chris", &[JSON], &body).await;
    assert_eq!(status, StatusCode::CREATED);
    exchange
}

/// The exchange `id`, as `@chris` sees it.
async fn exchange(pair: &Pair, id: &Value) -> Value {
    let path = format!("/exchanges/{}", id.as_str().unwrap());
    let (_, _, text) = request(&pair.app, "GET", &path, &pair.recipient, &[], "").await;
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn a_message_with_the_same_key_replaces_the_open_one() {
    let pair = pair().await;
    let earlier = keyed(&pair, &pair.sender, "disk").await;
    let later = keyed(&pair, &pair.sender, "disk").await;
    assert_eq!(
        (
            exchange(&pair, &earlier["id"]).await["state"].clone(),
            later["state"].clone(),
            later["replaces"].clone(),
            later["message"]["key"].clone(),
        ),
        (
            json!("replaced"),
            json!("open"),
            earlier["id"].clone(),
            json!("disk")
        )
    );
}

#[tokio::test]
async fn a_message_with_another_key_replaces_nothing() {
    let pair = pair().await;
    let earlier = keyed(&pair, &pair.sender, "disk").await;
    let later = keyed(&pair, &pair.sender, "memory").await;
    assert_eq!(
        (
            exchange(&pair, &earlier["id"]).await["state"].clone(),
            later["replaces"].clone()
        ),
        (json!("open"), Value::Null)
    );
}

#[tokio::test]
async fn another_senders_message_with_the_same_key_replaces_nothing() {
    let pair = pair().await;
    let other = individual(&pair.app, "ci-bot").await;
    send(&pair.app, &other, "/@chris", &[], "hello").await;
    decide(&pair.app, &pair.recipient, "chris", "allow").await;
    let earlier = keyed(&pair, &pair.sender, "disk").await;
    keyed(&pair, &other, "disk").await;
    assert_eq!(exchange(&pair, &earlier["id"]).await["state"], "open");
}

#[tokio::test]
async fn a_message_that_ended_is_not_replaced() {
    let pair = pair().await;
    let earlier = keyed(&pair, &pair.sender, "disk").await;
    let path = format!("/exchanges/{}/message", earlier["id"].as_str().unwrap());
    request(&pair.app, "DELETE", &path, &pair.sender, &[], "").await;
    let later = keyed(&pair, &pair.sender, "disk").await;
    assert_eq!(
        (
            exchange(&pair, &earlier["id"]).await["state"].clone(),
            later["replaces"].clone()
        ),
        (json!("withdrawn"), Value::Null)
    );
}

#[tokio::test]
async fn held_messages_with_one_key_replace_each_other_once_let_through() {
    let pair = pair().await;
    let stranger = individual(&pair.app, "grocery-bot").await;
    let first = keyed(&pair, &stranger, "milk").await;
    let second = keyed(&pair, &stranger, "milk").await;
    decide(&pair.app, &pair.recipient, "chris", "allow").await;
    let (first, second) = (
        exchange(&pair, &first["id"]).await,
        exchange(&pair, &second["id"]).await,
    );
    assert_eq!(
        (
            first["state"].clone(),
            second["state"].clone(),
            second["replaces"].clone()
        ),
        (json!("replaced"), json!("open"), first["id"].clone())
    );
}

#[tokio::test]
async fn a_key_arrives_as_a_header_or_a_form_field() {
    let pair = pair().await;
    let form = ("content-type", "application/x-www-form-urlencoded");
    let (_, by_header) = send(&pair.app, &pair.sender, "/@chris", &[("key", "disk")], "a").await;
    let (_, by_form) = send(
        &pair.app,
        &pair.sender,
        "/@chris",
        &[form],
        "body=b&key=disk",
    )
    .await;
    assert_eq!(
        (
            by_header["message"]["key"].clone(),
            by_form["replaces"].clone()
        ),
        (json!("disk"), by_header["id"].clone())
    );
}

#[tokio::test]
async fn an_empty_key_is_refused() {
    let pair = pair().await;
    let (status, _) = send(&pair.app, &pair.sender, "/@chris", &[("key", "")], "a").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// A second message with `@chris`'s first message's key, which replaces
/// it in `pipe`, and `pipe`'s replacement, not yet written.
async fn replacing_first(
    pair: &Pair,
    first: &Value,
    pipe: &mut redis::Pipeline,
) -> (
    crate::exchanges::records::Exchange,
    Option<crate::exchanges::replacing::Replacing>,
) {
    use crate::exchanges::{records, replacing, visible};
    let app = &pair.app.app;
    let mut connection = app.store.connection().await.unwrap();
    let chris: String = redis::cmd("HGET")
        .arg(app.store.key("names"))
        .arg("chris")
        .query_async(&mut connection)
        .await
        .unwrap();
    let mut later = visible(app, first["id"].as_str().unwrap(), &chris)
        .await
        .unwrap();
    later.id = records::new_id();
    pipe.atomic();
    records::create(pipe, &app.store, &later);
    let replacing = replacing::replace(app, &mut later, pipe).await.unwrap();
    (later, replacing)
}

#[tokio::test]
async fn a_replacement_announces_what_it_replaced_when_another_message_came_between() {
    let pair = pair().await;
    let first = keyed(&pair, &pair.sender, "disk").await;
    let mut pipe = redis::pipe();
    let (mut later, replacing) = replacing_first(&pair, &first, &mut pipe).await;
    let between = keyed(&pair, &pair.sender, "disk").await;
    crate::exchanges::replacing::write(&pair.app.app, &mut later, &pipe, replacing)
        .await
        .unwrap();
    let between = exchange(&pair, &between["id"]).await;
    assert_eq!(
        (later.replaces, between["state"].clone()),
        (between["id"].as_str().map(str::to_owned), json!("replaced"))
    );
}

#[tokio::test]
async fn a_replacement_names_nothing_when_what_it_expected_ended_between() {
    let pair = pair().await;
    let first = keyed(&pair, &pair.sender, "disk").await;
    let mut pipe = redis::pipe();
    let (mut later, replacing) = replacing_first(&pair, &first, &mut pipe).await;
    let withdraw = format!("/exchanges/{}/message", first["id"].as_str().unwrap());
    request(&pair.app, "DELETE", &withdraw, &pair.sender, &[], "").await;
    crate::exchanges::replacing::write(&pair.app.app, &mut later, &pipe, replacing)
        .await
        .unwrap();
    assert_eq!(later.replaces, None);
}
