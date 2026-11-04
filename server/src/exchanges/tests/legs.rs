use axum::http::StatusCode;
use serde_json::{Value, json};

use super::{JSON, individual, pair, question, send};
use crate::app::testing::request;

async fn get(pair: &super::Pair, token: &str, path: &str) -> (StatusCode, Value) {
    let (status, _, text) = request(&pair.app, "GET", path, token, &[], "").await;
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

async fn reply(
    pair: &super::Pair,
    token: &str,
    id: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (StatusCode, Value) {
    let path = format!("/exchanges/{id}/reply");
    let (status, _, text) = request(&pair.app, "PUT", &path, token, headers, body.to_owned()).await;
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

#[tokio::test]
async fn only_the_sender_sees_the_metadata() {
    let pair = pair().await;
    let id = question(&pair).await;
    let (_, as_sender) = get(&pair, &pair.sender, &format!("/exchanges/{id}")).await;
    let (_, as_recipient) = get(&pair, &pair.recipient, &format!("/exchanges/{id}")).await;
    assert_eq!(as_sender["message"]["metadata"], json!({"session": "abc"}));
    assert_eq!(as_recipient["message"].get("metadata"), None);
    assert_eq!(as_recipient["reply"], Value::Null);
}

#[tokio::test]
async fn strangers_cannot_see_an_exchange() {
    let pair = pair().await;
    let id = question(&pair).await;
    let stranger = individual(&pair.app, "stranger").await;
    let (status, _) = get(&pair, &stranger, &format!("/exchanges/{id}")).await;
    let (missing, _) = get(&pair, &stranger, "/exchanges/01NOPE").await;
    assert_eq!(
        (status, missing),
        (StatusCode::NOT_FOUND, StatusCode::NOT_FOUND)
    );
}

#[tokio::test]
async fn the_message_leg_has_its_own_url() {
    let pair = pair().await;
    let id = question(&pair).await;
    let (status, message) = get(&pair, &pair.recipient, &format!("/exchanges/{id}/message")).await;
    assert_eq!(
        (status, message["summary"].as_str()),
        (StatusCode::OK, Some("Restart postgres?"))
    );
}

#[tokio::test]
async fn a_recipient_replies_with_a_choice_and_text() {
    let pair = pair().await;
    let id = question(&pair).await;
    let form = ("content-type", "application/x-www-form-urlencoded");
    let (status, exchange) = reply(
        &pair,
        &pair.recipient,
        &id,
        &[form],
        "choice=Restart&text=after+5pm",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(exchange["state"], "replied");
    assert_eq!(exchange["reply"]["choice"], "restart");
    assert_eq!(exchange["reply"]["text"], "after 5pm");
    assert_eq!(exchange["reply"]["by"], "@chris");
}

#[tokio::test]
async fn plain_text_is_the_replys_text() {
    let pair = pair().await;
    let id = question(&pair).await;
    let (_, exchange) = reply(
        &pair,
        &pair.recipient,
        &id,
        &[],
        "don't do this at all ya dingus",
    )
    .await;
    assert_eq!(exchange["reply"]["text"], "don't do this at all ya dingus");
}

#[tokio::test]
async fn json_and_yaml_replies() {
    let pair = pair().await;
    let first = question(&pair).await;
    let second = question(&pair).await;
    let (_, by_json) = reply(
        &pair,
        &pair.recipient,
        &first,
        &[JSON],
        r#"{"choice": "Leave it"}"#,
    )
    .await;
    let yaml = ("content-type", "application/yaml");
    let (_, by_yaml) = reply(
        &pair,
        &pair.recipient,
        &second,
        &[yaml],
        "choice: restart\ntext: ok\n",
    )
    .await;
    assert_eq!(
        (
            by_json["reply"]["choice"].as_str(),
            by_yaml["reply"]["choice"].as_str()
        ),
        (Some("Leave it"), Some("restart"))
    );
}

#[tokio::test]
async fn a_notification_can_be_replied_to() {
    let pair = pair().await;
    let (_, exchange) = send(&pair.app, &pair.sender, "/@chris", &[], "Disk at 94%").await;
    let id = exchange["id"].as_str().unwrap();
    let (status, replied) = reply(&pair, &pair.recipient, id, &[], "looking into it").await;
    assert_eq!(
        (status, replied["state"].as_str()),
        (StatusCode::OK, Some("replied"))
    );
}

#[tokio::test]
async fn bad_replies_are_refused() {
    let pair = pair().await;
    let id = question(&pair).await;
    let bad: [(&[(&str, &str)], &str); 4] = [
        (&[JSON], r#"{"choice": "Explode"}"#),
        (&[JSON], "{}"),
        (&[JSON], r#"{"choice": 1}"#),
        (&[("content-type", "application/yaml")], "choice: [unclosed"),
    ];
    for (headers, body) in bad {
        let (status, _) = reply(&pair, &pair.recipient, &id, headers, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    let (sender, _) = reply(&pair, &pair.sender, &id, &[], "I'll answer myself").await;
    assert_eq!(sender, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_first_reply_wins() {
    let pair = pair().await;
    let id = question(&pair).await;
    reply(
        &pair,
        &pair.recipient,
        &id,
        &[JSON],
        r#"{"choice": "restart"}"#,
    )
    .await;
    let (again, _) = reply(
        &pair,
        &pair.recipient,
        &id,
        &[JSON],
        r#"{"choice": "restart"}"#,
    )
    .await;
    let (different, conflict) = reply(
        &pair,
        &pair.recipient,
        &id,
        &[JSON],
        r#"{"choice": "Leave it"}"#,
    )
    .await;
    let (strict, _) = reply(
        &pair,
        &pair.recipient,
        &id,
        &[JSON, ("if-none-match", "*")],
        r#"{"choice": "restart"}"#,
    )
    .await;
    assert_eq!(
        (again, different, strict),
        (
            StatusCode::OK,
            StatusCode::CONFLICT,
            StatusCode::PRECONDITION_FAILED
        )
    );
    assert_eq!(conflict["reply"]["choice"], "restart");
}

#[tokio::test]
async fn the_reply_leg_has_its_own_url() {
    let pair = pair().await;
    let id = question(&pair).await;
    let (before, _) = get(&pair, &pair.sender, &format!("/exchanges/{id}/reply")).await;
    reply(&pair, &pair.recipient, &id, &[], "no").await;
    let (after, found) = get(&pair, &pair.sender, &format!("/exchanges/{id}/reply")).await;
    assert_eq!((before, after), (StatusCode::NOT_FOUND, StatusCode::OK));
    assert_eq!(found["text"], "no");
}

#[tokio::test]
async fn the_sender_withdraws_a_message() {
    let pair = pair().await;
    let id = question(&pair).await;
    let path = format!("/exchanges/{id}/message");
    let (by_recipient, _, _) = request(&pair.app, "DELETE", &path, &pair.recipient, &[], "").await;
    let (withdrawn, _, _) = request(&pair.app, "DELETE", &path, &pair.sender, &[], "").await;
    let (again, _, _) = request(&pair.app, "DELETE", &path, &pair.sender, &[], "").await;
    let (gone, message) = get(&pair, &pair.recipient, &path).await;
    let (late_reply, _) = reply(&pair, &pair.recipient, &id, &[], "too late").await;
    assert_eq!(
        [by_recipient, withdrawn, again, gone, late_reply],
        [
            StatusCode::FORBIDDEN,
            StatusCode::OK,
            StatusCode::OK,
            StatusCode::GONE,
            StatusCode::CONFLICT
        ]
    );
    assert_eq!(message["summary"], "Restart postgres?");
}

#[tokio::test]
async fn a_replied_message_cannot_be_withdrawn() {
    let pair = pair().await;
    let id = question(&pair).await;
    reply(&pair, &pair.recipient, &id, &[], "done").await;
    let path = format!("/exchanges/{id}/message");
    let (status, _, _) = request(&pair.app, "DELETE", &path, &pair.sender, &[], "").await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn read_state_is_private_to_each_recipient() {
    let pair = pair().await;
    let id = question(&pair).await;
    let mine = format!("/exchanges/{id}/@chris");
    let (_, before) = get(&pair, &pair.recipient, &mine).await;
    request(&pair.app, "PUT", &mine, &pair.recipient, &[], "").await;
    let (_, read) = get(&pair, &pair.recipient, &mine).await;
    request(&pair.app, "DELETE", &mine, &pair.recipient, &[], "").await;
    let (_, unread) = get(&pair, &pair.recipient, &mine).await;
    assert_eq!(
        [&before, &read, &unread].map(|state| state["read"].as_bool()),
        [Some(false), Some(true), Some(false)]
    );
    let (by_sender, _) = get(&pair, &pair.sender, &mine).await;
    let (senders_own, _) = get(&pair, &pair.sender, &format!("/exchanges/{id}/@review-bot")).await;
    let (no_at, _) = get(&pair, &pair.recipient, &format!("/exchanges/{id}/chris")).await;
    assert_eq!(
        [by_sender, senders_own, no_at],
        [
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::NOT_FOUND
        ]
    );
}

#[tokio::test]
async fn a_reply_that_is_not_utf8_is_a_bad_request() {
    let pair = pair().await;
    let id = question(&pair).await;
    let path = format!("/exchanges/{id}/reply");
    for content_type in ["text/plain", "application/yaml"] {
        let headers = [("content-type", content_type)];
        let (status, _, _) = request(
            &pair.app,
            "PUT",
            &path,
            &pair.recipient,
            &headers,
            vec![0xff, 0xfe],
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{content_type}");
    }
}
