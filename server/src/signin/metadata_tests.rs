use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{Value, json};

use super::browser_tests::browser_app;
use super::test_issuer::TestIssuer;
use super::{Issuer, Issuers};
use crate::app::App;
use crate::app::testing::{TestApp, request};
use crate::store::testing::store;

const ORIGIN: &str = "https://transom.example.com";

async fn metadata(app: &TestApp, path: &str) -> Value {
    let (status, _, body) = request(app, "GET", path, "", &[], "").await;
    assert_eq!(status, StatusCode::OK);
    serde_json::from_str(&body).unwrap()
}

#[tokio::test]
async fn describes_the_mcp_server() {
    let app = browser_app(None, Some(ORIGIN)).await;
    let metadata = metadata(&app, "/.well-known/oauth-protected-resource/mcp").await;
    assert_eq!(
        metadata,
        json!({
            "resource": "https://transom.example.com/mcp",
            "authorization_servers": [app.issuer.url],
            "bearer_methods_supported": ["header"],
            "scopes_supported": ["openid", "profile", "email", "offline_access"],
            "resource_name": "transom",
        })
    );
}

#[tokio::test]
async fn describes_the_http_api_at_the_root() {
    let app = browser_app(None, Some(ORIGIN)).await;
    let metadata = metadata(&app, "/.well-known/oauth-protected-resource").await;
    assert_eq!(metadata["resource"], "https://transom.example.com");
}

#[tokio::test]
async fn names_the_host_without_a_public_url() {
    let app = browser_app(None, None).await;
    let path = "/.well-known/oauth-protected-resource/mcp";
    let host = [("host", "transom.local:8080")];
    let (_, _, body) = request(&app, "GET", path, "", &host, "").await;
    let metadata: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(metadata["resource"], "http://transom.local:8080/mcp");
}

#[tokio::test]
async fn names_only_the_issuers_people_sign_in_with() {
    let people = TestIssuer::start().await;
    let workloads = TestIssuer::start().await;
    let issuers = Issuers::of(vec![
        Issuer::new(people.browser_settings(None)).unwrap(),
        Issuer::new(workloads.settings()).unwrap(),
    ]);
    let app = TestApp {
        app: Arc::new(App::new(store(), issuers)),
        issuer: Arc::new(people),
    };
    let metadata = metadata(&app, "/.well-known/oauth-protected-resource/mcp").await;
    assert_eq!(metadata["authorization_servers"], json!([app.issuer.url]));
}

#[tokio::test]
async fn may_be_cached() {
    let app = browser_app(None, Some(ORIGIN)).await;
    let path = "/.well-known/oauth-protected-resource/mcp";
    let (_, headers, _) = request(&app, "GET", path, "", &[], "").await;
    assert_eq!(headers["cache-control"], "max-age=3600");
}

async fn challenge(app: &TestApp, method: &str, path: &str, token: &str) -> String {
    let headers = [
        ("content-type", "application/json"),
        ("accept", "application/json, text/event-stream"),
    ];
    let (status, headers, _) = request(app, method, path, token, &headers, "{}").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    headers["www-authenticate"].to_str().unwrap().to_owned()
}

#[tokio::test]
async fn points_an_mcp_client_without_a_token_to_the_mcp_servers_metadata() {
    let app = browser_app(None, Some(ORIGIN)).await;
    assert_eq!(
        challenge(&app, "POST", "/mcp", "").await,
        "Bearer realm=\"transom\", resource_metadata=\"https://transom.example.com/.well-known/oauth-protected-resource/mcp\""
    );
}

#[tokio::test]
async fn points_an_mcp_client_with_a_bad_token_to_the_mcp_servers_metadata() {
    let app = browser_app(None, Some(ORIGIN)).await;
    assert_eq!(
        challenge(&app, "POST", "/mcp", "garbage").await,
        "Bearer realm=\"transom\", error=\"invalid_token\", resource_metadata=\"https://transom.example.com/.well-known/oauth-protected-resource/mcp\""
    );
}

#[tokio::test]
async fn points_an_http_client_without_a_token_to_the_apis_metadata() {
    let app = browser_app(None, Some(ORIGIN)).await;
    assert_eq!(
        challenge(&app, "GET", "/@chris", "").await,
        "Bearer realm=\"transom\", resource_metadata=\"https://transom.example.com/.well-known/oauth-protected-resource\""
    );
}
