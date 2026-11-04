//! The MCP server over its real transport, Streamable HTTP, spoken by hand.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::app::App;
use crate::app::testing::{TestApp, decide, request};

/// One MCP session, as `token`, against the app's /mcp. Its router is
/// built once, since the router holds the sessions.
struct Session {
    app: Arc<App>,
    router: axum::Router,
    token: String,
    id: String,
}

fn mcp(app: &Arc<App>) -> axum::Router {
    super::router_with(app.clone(), Duration::from_millis(50)).with_state(app.clone())
}

/// The JSON-RPC messages in an MCP response, which is JSON or a stream of
/// Server-Sent Events.
fn messages(body: &str) -> Vec<Value> {
    if let Ok(value) = serde_json::from_str(body) {
        return vec![value];
    }
    body.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|data| serde_json::from_str(data.trim()).ok())
        .collect()
}

async fn post(
    router: &axum::Router,
    token: &str,
    session: Option<&str>,
    message: &Value,
) -> (StatusCode, Option<String>, Vec<Value>) {
    let mut request = Request::post("/mcp")
        .header("host", "transom.example.com")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
    if let Some(session) = session {
        request = request.header("mcp-session-id", session);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::from(message.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let id = response
        .headers()
        .get("mcp-session-id")
        .map(|value| value.to_str().unwrap().to_owned());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, id, messages(std::str::from_utf8(&body).unwrap()))
}

impl Session {
    async fn open(app: &Arc<App>, token: &str) -> Self {
        let initialize = json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}},
        });
        let router = mcp(app);
        let (status, id, _) = post(&router, token, None, &initialize).await;
        assert_eq!(status, StatusCode::OK);
        let id = id.unwrap();
        let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        post(&router, token, Some(&id), &initialized).await;
        Self {
            app: app.clone(),
            router,
            token: token.to_owned(),
            id,
        }
    }

    /// Calls `tool` with `arguments`, and returns every message the call
    /// sent: progress notifications, then the result.
    async fn call_all(&self, tool: &str, arguments: Value, progress: bool) -> Vec<Value> {
        let mut params = json!({ "name": tool, "arguments": arguments });
        if progress {
            params["_meta"] = json!({ "progressToken": "waiting" });
        }
        let call = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": params });
        post(&self.router, &self.token, Some(&self.id), &call)
            .await
            .2
    }

    /// Calls `tool`, and returns its result's text as JSON, and whether it
    /// is an error.
    async fn call(&self, tool: &str, arguments: Value) -> (Value, bool) {
        let messages = self.call_all(tool, arguments, false).await;
        let result = &messages.last().unwrap()["result"];
        let text = result["content"][0]["text"].as_str().unwrap();
        (
            serde_json::from_str(text).unwrap_or(Value::String(text.to_owned())),
            result["isError"] == true,
        )
    }
}

async fn individual(app: &TestApp, name: &str) -> String {
    let token = app.issuer.token(&ulid::Ulid::generate().to_string());
    request(app, "PUT", &format!("/@{name}"), &token, &[], "").await;
    token
}

/// `@review-bot`, asking over MCP, and `@chris`, who has allowed it.
async fn pair() -> (TestApp, Session, String) {
    let app = TestApp::new().await;
    let agent = individual(&app, "review-bot").await;
    let chris = individual(&app, "chris").await;
    let session = Session::open(&app.app, &agent).await;
    session
        .call("notify", json!({ "to": "@chris", "summary": "hello" }))
        .await;
    decide(&app, &chris, "chris", "allow").await;
    (app, session, chris)
}

#[tokio::test]
async fn signing_in_is_required() {
    let app = TestApp::new().await;
    let (status, _, _) = post(&mcp(&app.app), "not-a-token", None, &json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn lists_its_six_tools() {
    let app = TestApp::new().await;
    let agent = individual(&app, "review-bot").await;
    let session = Session::open(&app.app, &agent).await;
    let list = json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" });
    let messages = post(&session.router, &agent, Some(&session.id), &list)
        .await
        .2;
    let mut names: Vec<_> = messages[0]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["ask", "check", "inbox", "notify", "reply", "withdraw"]
    );
}

#[tokio::test]
async fn asks_and_returns_at_once() {
    let (_, session, _) = pair().await;
    let (exchange, error) = session
        .call(
            "ask",
            json!({ "to": "@chris", "summary": "Merge PR #12?", "choices": ["Merge", "Hold"] }),
        )
        .await;
    assert!(!error);
    assert_eq!(
        (exchange["state"].as_str(), exchange["question"].as_bool()),
        (Some("open"), Some(true))
    );
}

#[tokio::test]
async fn asks_and_waits_for_the_reply_reporting_progress() {
    let (app, session, chris) = pair().await;
    let asking = session.call_all(
        "ask",
        json!({ "to": "@chris", "summary": "Merge PR #12?", "choices": ["Merge", "Hold"], "wait": 10 }),
        true,
    );
    let replying = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let (_, _, list) = request(
            &app,
            "GET",
            "/@chris/exchanges?question=true&state=open",
            &chris,
            &[],
            "",
        )
        .await;
        let list: Value = serde_json::from_str(&list).unwrap();
        let id = list["exchanges"][0]["id"].as_str().unwrap();
        request(
            &app,
            "PUT",
            &format!("/exchanges/{id}/reply"),
            &chris,
            &[],
            "Merge it",
        )
        .await;
    };
    let (messages, ()) = tokio::join!(asking, replying);
    assert!(
        messages
            .iter()
            .any(|message| message["method"] == "notifications/progress")
    );
    let text = messages.last().unwrap()["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let exchange: Value = serde_json::from_str(text).unwrap();
    assert_eq!(exchange["reply"]["text"], "Merge it");
}

#[tokio::test]
async fn a_wait_that_runs_out_returns_the_open_exchange() {
    let (_, session, _) = pair().await;
    let (exchange, _) = session
        .call(
            "ask",
            json!({ "to": "@chris", "summary": "Anyone?", "prompt": "", "wait": 1 }),
        )
        .await;
    assert_eq!(exchange["state"], "open");
}

#[tokio::test]
async fn checks_replies_withdraws_and_lists() {
    let (_, session, chris) = pair().await;
    let (asked, _) = session
        .call(
            "ask",
            json!({ "to": "@chris", "summary": "Deploy?", "choices": ["Yes"] }),
        )
        .await;
    let id = asked["id"].as_str().unwrap();
    let (checked, _) = session.call("check", json!({ "exchange": id })).await;
    let (waited, _) = session
        .call("check", json!({ "exchange": id, "wait": 1 }))
        .await;
    assert_eq!(
        (checked["state"].as_str(), waited["state"].as_str()),
        (Some("open"), Some("open"))
    );
    let (withdrawn, _) = session.call("withdraw", json!({ "exchange": id })).await;
    let (ended, _) = session
        .call("check", json!({ "exchange": id, "wait": 10 }))
        .await;
    assert_eq!(
        (withdrawn["state"].as_str(), ended["state"].as_str()),
        (Some("withdrawn"), Some("withdrawn"))
    );
    let chris_session = Session::open(&session.app, &chris).await;
    session
        .call("notify", json!({ "to": "@chris", "summary": "FYI" }))
        .await;
    let (inbox, _) = chris_session
        .call(
            "inbox",
            json!({ "question": false, "state": "open", "limit": 1 }),
        )
        .await;
    let notification = inbox["exchanges"][0]["id"].as_str().unwrap();
    let (replied, _) = chris_session
        .call(
            "reply",
            json!({ "exchange": notification, "text": "thanks" }),
        )
        .await;
    assert_eq!(replied["state"], "replied");
}

#[tokio::test]
async fn reports_what_went_wrong_as_an_error_result() {
    let (_, session, _) = pair().await;
    let calls = [
        ("ask", json!({ "to": "@nobody", "summary": "Hello?" })),
        ("ask", json!({ "to": "@chris" })),
        ("check", json!({ "exchange": "01NOPE" })),
        ("reply", json!({ "exchange": "01NOPE", "text": "hi" })),
        ("withdraw", json!({ "exchange": "01NOPE" })),
        ("inbox", json!({ "state": "lost" })),
    ];
    for (tool, arguments) in calls {
        let (_, error) = session.call(tool, arguments.clone()).await;
        assert!(error, "{tool} {arguments}");
    }
}

#[tokio::test]
async fn refuses_a_name_that_has_moved() {
    let (app, session, chris) = pair().await;
    let rename = json!({ "name": "guid.foo" }).to_string();
    request(
        &app,
        "PATCH",
        "/@chris",
        &chris,
        &[("content-type", "application/json")],
        rename,
    )
    .await;
    let (moved, error) = session
        .call("notify", json!({ "to": "@chris", "summary": "Hello?" }))
        .await;
    assert!(error, "{moved}");
}
