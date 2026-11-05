//! The exchanges' HTTP interface, end to end against a real Redis and a
//! real token issuer.

mod attachments;
mod consent;
mod feed;
mod legs;
mod listing;
mod marking;
mod parties;
mod replacing;
mod replicas;
mod retention;
mod sending;
mod waiting;

use axum::http::StatusCode;
use serde_json::Value;

pub(super) use crate::app::testing::decide;
use crate::app::testing::{TestApp, request};

/// Signs in a new individual named `name` and returns its token.
pub(super) async fn individual(app: &TestApp, name: &str) -> String {
    let token = app.issuer.token(&ulid::Ulid::generate().to_string());
    let (status, _, _) = request(app, "PUT", &format!("/@{name}"), &token, &[], "").await;
    assert_eq!(status, StatusCode::CREATED);
    token
}

/// Sends `body` with `headers` from `token` to `to`, and returns the status
/// and the exchange as JSON.
pub(super) async fn send(
    app: &TestApp,
    token: &str,
    to: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (StatusCode, Value) {
    let (status, _, text) = request(app, "POST", to, token, headers, body.to_owned()).await;
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

pub(super) const JSON: (&str, &str) = ("content-type", "application/json");

/// The id of the newest open question `token` can see in `@{name}`'s
/// list, waiting for one to appear.
pub(super) async fn newest(app: &TestApp, token: &str, name: &str) -> String {
    let path = format!("/@{name}/exchanges?question=true&state=open");
    for _ in 0..100 {
        let (_, _, text) = request(app, "GET", &path, token, &[], "").await;
        let list: Value = serde_json::from_str(&text).unwrap();
        if let Some(id) = list["exchanges"][0]["id"].as_str() {
            return id.to_owned();
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("no exchange appeared");
}

/// Two signed-in individuals, a sender and a recipient.
pub(super) struct Pair {
    pub(super) app: TestApp,
    pub(super) sender: String,
    pub(super) recipient: String,
}

/// `@review-bot` and `@chris`, with `@chris` having allowed `@review-bot`.
pub(super) async fn pair() -> Pair {
    let app = TestApp::new().await;
    let sender = individual(&app, "review-bot").await;
    let recipient = individual(&app, "chris").await;
    send(&app, &sender, "/@chris", &[], "hello").await;
    decide(&app, &recipient, "chris", "allow").await;
    Pair {
        app,
        sender,
        recipient,
    }
}

/// Sends a question with two choices and returns its exchange's id.
pub(super) async fn question(pair: &Pair) -> String {
    let body = r#"{"summary": "Restart postgres?", "choices": ["Leave it", {"label": "Restart", "id": "restart"}], "metadata": {"session": "abc"}}"#;
    let (status, exchange) = send(&pair.app, &pair.sender, "/@chris", &[JSON], body).await;
    assert_eq!(status, StatusCode::CREATED);
    exchange["id"].as_str().unwrap().to_owned()
}

/// `@chris` in a group `@household`, and `@spouse`, who is not in it.
pub(super) async fn household() -> (TestApp, String, String) {
    let app = TestApp::new().await;
    let chris = individual(&app, "chris").await;
    let spouse = individual(&app, "spouse").await;
    request(&app, "PUT", "/@household", &chris, &[], "").await;
    (app, chris, spouse)
}
