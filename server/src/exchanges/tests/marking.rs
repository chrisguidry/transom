//! Marking many exchanges read at once, at `POST /@{name}/read`.

use axum::http::StatusCode;
use serde_json::{Value, json};

use super::{JSON, Pair, decide, household, individual, pair, send};
use crate::app::testing::{TestApp, events, request};

const FORM: (&str, &str) = ("content-type", "application/x-www-form-urlencoded");
const YAML: (&str, &str) = ("content-type", "application/yaml");

/// Sends `@chris` three notifications from `@review-bot`, and returns
/// their ids, oldest first.
async fn three(pair: &Pair) -> [String; 3] {
    let mut ids = Vec::new();
    for body in ["first", "second", "third"] {
        let (_, exchange) = send(&pair.app, &pair.sender, "/@chris", &[], body).await;
        ids.push(exchange["id"].as_str().unwrap().to_owned());
    }
    ids.try_into().unwrap()
}

/// Posts `body` to `/@chris/read` as `@chris`, and returns the status and
/// the answer as JSON.
async fn mark(pair: &Pair, headers: &[(&str, &str)], body: String) -> (StatusCode, Value) {
    let (status, _, text) = request(
        &pair.app,
        "POST",
        "/@chris/read",
        &pair.recipient,
        headers,
        body,
    )
    .await;
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

/// Marks `ids` read as `@chris`, and returns the ids the answer says it
/// marked.
async fn mark_ids(pair: &Pair, ids: &[&str]) -> Value {
    let body = json!({ "exchanges": ids }).to_string();
    let (status, answer) = mark(pair, &[JSON], body).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    answer["read"].clone()
}

/// Which of `ids` `@chris` has not read.
async fn unread_of(pair: &Pair, ids: &[String]) -> Vec<String> {
    let (_, _, text) = request(
        &pair.app,
        "GET",
        "/@chris/exchanges?read=false",
        &pair.recipient,
        &[],
        "",
    )
    .await;
    let list: Value = serde_json::from_str(&text).unwrap();
    let listed: Vec<&str> = list["exchanges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|exchange| exchange["id"].as_str().unwrap())
        .collect();
    ids.iter()
        .filter(|id| listed.contains(&id.as_str()))
        .cloned()
        .collect()
}

/// Marks the first two of three notifications read with a body that
/// `body` writes from their ids, under `content_type`. Returns what the
/// answer says it marked, and which of the three are still unread, each
/// as positions among the three.
async fn marking_two_of_three(
    content_type: (&str, &str),
    body: fn(&str, &str) -> String,
) -> (Vec<usize>, Vec<usize>) {
    let pair = pair().await;
    let ids = three(&pair).await;
    let (status, answer) = mark(&pair, &[content_type], body(&ids[0], &ids[1])).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    let position = |id: &str| ids.iter().position(|one| one == id).unwrap();
    let marked = answer["read"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| position(id.as_str().unwrap()))
        .collect();
    let unread = unread_of(&pair, &ids)
        .await
        .iter()
        .map(|id| position(id))
        .collect();
    (marked, unread)
}

#[tokio::test]
async fn marks_the_exchanges_a_body_names() {
    let results = [
        marking_two_of_three(JSON, |a, b| json!({ "exchanges": [a, b] }).to_string()).await,
        marking_two_of_three(FORM, |a, b| format!("exchanges={a}&exchanges={b}")).await,
        marking_two_of_three(YAML, |a, b| format!("exchanges: [{a}, {b}]")).await,
    ];
    assert_eq!(results, std::array::from_fn(|_| (vec![0, 1], vec![2])));
}

#[tokio::test]
async fn marks_what_arrived_up_to_a_time() {
    let pair = pair().await;
    let ids = three(&pair).await;
    let (_, _, text) = request(
        &pair.app,
        "GET",
        &format!("/exchanges/{}", ids[1]),
        &pair.recipient,
        &[],
        "",
    )
    .await;
    let second: Value = serde_json::from_str(&text).unwrap();
    let body = format!("before={}", second["created"].as_str().unwrap());
    let (status, answer) = mark(&pair, &[FORM], body).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(unread_of(&pair, &ids).await, [ids[2].clone()]);
}

#[tokio::test]
async fn marks_what_arrived_for_a_group_up_to_a_time() {
    let (app, chris, spouse) = household().await;
    let (_, exchange) = send(&app, &spouse, "/@household", &[], "dinner?").await;
    decide(&app, &chris, "household", "allow").await;
    let id = exchange["id"].as_str().unwrap();
    let body = json!({ "before": jiff::Timestamp::now().to_string() }).to_string();
    let (_, _, text) = request(&app, "POST", "/@chris/read", &chris, &[JSON], body).await;
    let answer: Value = serde_json::from_str(&text).unwrap();
    assert!(
        answer["read"]
            .as_array()
            .unwrap()
            .contains(&Value::from(id)),
        "{answer}"
    );
}

#[tokio::test]
async fn answers_only_what_it_changed() {
    let pair = pair().await;
    let ids = three(&pair).await;
    mark_ids(&pair, &[&ids[0]]).await;
    let answer = mark_ids(&pair, &[&ids[0], &ids[1], &ids[1]]).await;
    assert_eq!(answer, json!([ids[1]]));
}

#[tokio::test]
async fn leaves_out_what_the_caller_did_not_receive() {
    let pair = pair().await;
    let [first, ..] = three(&pair).await;
    let (_, sent) = send(&pair.app, &pair.recipient, "/@review-bot", &[], "hi").await;
    let sent = sent["id"].as_str().unwrap();
    let answer = mark_ids(&pair, &[sent, "01JBNOSUCHEXCHANGE0000000", &first]).await;
    assert_eq!(answer, json!([first]));
}

#[tokio::test]
async fn each_read_reaches_the_readers_feed() {
    let pair = pair().await;
    let ids = three(&pair).await;
    let marking = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        mark_ids(&pair, &[&ids[0], &ids[1]]).await;
    };
    let seen = events(&pair.app, "/@chris/exchanges", &pair.recipient, 2, marking).await;
    let reads: Vec<(&str, &str, bool)> = seen
        .iter()
        .map(|(name, data)| {
            (
                name.as_str(),
                data["id"].as_str().unwrap(),
                data["read"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        reads,
        [
            ("exchange", ids[0].as_str(), true),
            ("exchange", ids[1].as_str(), true)
        ]
    );
}

/// The status of a POST of `body` to `path` as `@chris`.
async fn refusal(path: &str, headers: &[(&str, &str)], body: &str) -> StatusCode {
    let app = TestApp::new().await;
    let chris = individual(&app, "chris").await;
    individual(&app, "review-bot").await;
    request(&app, "POST", path, &chris, headers, body.to_owned())
        .await
        .0
}

#[tokio::test]
async fn refuses_what_it_cannot_do() {
    let statuses = [
        refusal("/@review-bot/read", &[JSON], r#"{"exchanges": []}"#).await,
        refusal("/chris/read", &[JSON], r#"{"exchanges": []}"#).await,
        refusal("/@chris/read", &[JSON], "{}").await,
        refusal(
            "/@chris/read",
            &[JSON],
            r#"{"exchanges": [], "before": "2026-11-05T12:00:00Z"}"#,
        )
        .await,
        refusal("/@chris/read", &[JSON], r#"{"before": "yesterday"}"#).await,
        refusal("/@chris/read", &[JSON], r#"{"ids": []}"#).await,
        refusal("/@chris/read", &[YAML], "exchanges: {}").await,
        refusal("/@chris/read", &[FORM], "id=01JB").await,
        refusal("/@chris/read", &[], "everything").await,
    ];
    assert_eq!(
        statuses,
        [
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_REQUEST,
        ]
    );
}
