use axum::http::StatusCode;
use serde_json::Value;

use super::{individual, pair, question, send};
use crate::app::testing::{events, request};

async fn list(pair: &super::Pair, token: &str, path: &str) -> (StatusCode, Value) {
    let (status, _, text) = request(&pair.app, "GET", path, token, &[], "").await;
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

fn ids(list: &Value) -> Vec<String> {
    list["exchanges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|exchange| exchange["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn exchange_ids_increase_within_a_millisecond() {
    let ids: Vec<String> = (0..1000)
        .map(|_| crate::exchanges::records::new_id())
        .collect();
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
}

#[tokio::test]
async fn an_identity_lists_what_it_sent_and_received_newest_first() {
    let pair = pair().await;
    let first = question(&pair).await;
    let (_, second) = send(&pair.app, &pair.recipient, "/@review-bot", &[], "thanks").await;
    let (_, mine) = list(&pair, &pair.recipient, "/@chris/exchanges?limit=2").await;
    assert_eq!(
        ids(&mine),
        [second["id"].as_str().unwrap().to_owned(), first]
    );
}

#[tokio::test]
async fn the_list_filters() {
    let pair = pair().await;
    let open = question(&pair).await;
    let (_, notification) = send(&pair.app, &pair.sender, "/@chris", &[], "FYI").await;
    let notification = notification["id"].as_str().unwrap().to_owned();
    let mine = format!("/exchanges/{notification}/@chris");
    request(&pair.app, "PUT", &mine, &pair.recipient, &[], "").await;
    let (_, questions) = list(
        &pair,
        &pair.recipient,
        "/@chris/exchanges?question=true&state=open",
    )
    .await;
    let (_, unread) = list(
        &pair,
        &pair.recipient,
        "/@chris/exchanges?read=false&limit=1",
    )
    .await;
    let (_, by_state) = list(
        &pair,
        &pair.recipient,
        "/@chris/exchanges?state=open&limit=1",
    )
    .await;
    assert_eq!(ids(&questions), std::slice::from_ref(&open));
    assert_eq!(ids(&unread), [open]);
    assert_eq!(ids(&by_state), [notification]);
    let (bad, _) = list(&pair, &pair.recipient, "/@chris/exchanges?state=lost").await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_identitys_list_is_its_own() {
    let pair = pair().await;
    let (status, _) = list(&pair, &pair.sender, "/@chris/exchanges").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_members_list_includes_its_groups_exchanges() {
    let pair = pair().await;
    request(&pair.app, "PUT", "/@household", &pair.recipient, &[], "").await;
    let (_, sent) = send(&pair.app, &pair.sender, "/@household", &[], "Dinner at 6").await;
    let id = sent["id"].as_str().unwrap().to_owned();
    super::decide(&pair.app, &pair.recipient, "household", "allow").await;
    let (_, mine) = list(&pair, &pair.recipient, "/@chris/exchanges").await;
    let (_, groups) = list(&pair, &pair.recipient, "/@household/exchanges?state=open").await;
    assert!(ids(&mine).contains(&id));
    assert_eq!(ids(&groups), std::slice::from_ref(&id));
    let path = format!("/exchanges/{id}/reply");
    let (status, _, _) = request(&pair.app, "PUT", &path, &pair.recipient, &[], "yum").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_list_at_a_former_name_redirects() {
    let pair = pair().await;
    let rename = serde_json::json!({"name": "guid.foo"}).to_string();
    request(
        &pair.app,
        "PATCH",
        "/@chris",
        &pair.recipient,
        &[super::JSON],
        rename,
    )
    .await;
    let (status, _, _) = request(
        &pair.app,
        "GET",
        "/@chris/exchanges",
        &pair.recipient,
        &[],
        "",
    )
    .await;
    assert_eq!(status, StatusCode::PERMANENT_REDIRECT);
}

#[tokio::test]
async fn the_live_feed_announces_new_and_changed_exchanges() {
    let pair = pair().await;
    let stranger = individual(&pair.app, "stranger").await;
    let happening = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
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
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
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
    let bodies: Vec<_> = seen
        .iter()
        .map(|(_, exchange)| exchange["message"]["body"].as_str())
        .collect();
    assert_eq!(bodies, [None, Some("and another")]);
}
