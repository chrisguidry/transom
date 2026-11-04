//! The app's sign-in end to end: the Custom Tab's trip through the issuer,
//! the one-time code at the app's callback, trading it for a session, and
//! using and ending that session as a bearer token.

use std::sync::Arc;

use axum::http::StatusCode;
use redis::AsyncCommands;
use serde_json::{Value, json};

use super::browser_tests::{browser_app, query, set_cookie};
use super::sessions::hashed;
use super::{Issuer, Issuers};
use crate::app::App;
use crate::app::testing::{TestApp, request};

const CALLBACK: &str = "https://transom.sh/app/callback";

const VERIFIER: &str = "the-verifier-the-app-made-and-kept-to-itself";

/// The query that begins a sign-in for the app, with the challenge of
/// `VERIFIER`.
fn app_query() -> String {
    format!("app={}&callback={CALLBACK}", hashed(VERIFIER))
}

/// Signs in as `subject` named `name` from the app's Custom Tab, and
/// returns where transom sent the Custom Tab, and whether it set a session
/// cookie.
async fn sign_in(app: &TestApp, subject: &str, name: &str) -> (String, bool) {
    let (_, headers, _) = request(
        app,
        "GET",
        &format!("/login/test?{}", app_query()),
        "",
        &[],
        "",
    )
    .await;
    let location = headers["location"].to_str().unwrap();
    let (state, nonce) = (query(location, "state"), query(location, "nonce"));
    let login = set_cookie(&headers, "__Host-transom-login").unwrap();
    let path = format!("/login/callback?code={subject}%7C{name}%7C{nonce}&state={state}");
    let cookie = format!("__Host-transom-login={login}");
    let (status, headers, _) = request(app, "GET", &path, "", &[("cookie", &cookie)], "").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let session_cookie = set_cookie(&headers, "__Host-Http-transom").is_some();
    (
        headers["location"].to_str().unwrap().to_owned(),
        session_cookie,
    )
}

/// Trades `code` and `verifier` at `/login/app`.
async fn redeem(app: &TestApp, code: &str, verifier: &str) -> (StatusCode, Value) {
    let body = json!({ "code": code, "verifier": verifier }).to_string();
    let (status, _, text) = request(
        app,
        "POST",
        "/login/app",
        "",
        &[("content-type", "application/json")],
        body,
    )
    .await;
    (status, serde_json::from_str(&text).unwrap())
}

/// Signs `@chris` in from the app, and returns the session's token.
async fn signed_in(app: &TestApp) -> String {
    let (location, _) = sign_in(app, "chris-subject", "chris").await;
    let (_, body) = redeem(app, &query(&location, "code"), VERIFIER).await;
    body["token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn sends_the_custom_tab_to_the_app_with_a_code_and_no_cookie() {
    let app = browser_app(None, None).await;
    let (location, session_cookie) = sign_in(&app, "chris-subject", "chris").await;
    assert!(
        location.starts_with(&format!("{CALLBACK}?code=")),
        "{location}"
    );
    assert!(!session_cookie);
}

#[tokio::test]
async fn the_app_holds_its_session_as_a_bearer_token() {
    let app = browser_app(None, None).await;
    let token = signed_in(&app).await;
    let (status, _, text) = request(&app, "GET", "/@chris", &token, &[], "").await;
    assert!(token.starts_with("transom_"), "{token}");
    assert_eq!(status, StatusCode::OK, "{text}");
}

#[tokio::test]
async fn the_sign_in_page_carries_the_app_sign_in_to_each_issuer() {
    let app = browser_app(None, None).await;
    let path = format!("/login?{}", app_query());
    let (_, _, page) = request(&app, "GET", &path, "", &[("accept", "text/html")], "").await;
    let link = format!(
        "href=\"/login/test?next=/&amp;app={}&amp;callback=https%3A//transom.sh/app/callback\"",
        hashed(VERIFIER)
    );
    assert!(page.contains(&link), "{page}");
}

#[tokio::test]
async fn refuses_to_begin_an_app_sign_in_it_does_not_allow() {
    let app = browser_app(None, None).await;
    let challenge = hashed(VERIFIER);
    let cases = [
        format!("/login/test?app={challenge}&callback=https://evil.example.com/callback"),
        format!("/login/test?app={challenge}&callback=not-a-url"),
        format!("/login/test?app={challenge}"),
        format!("/login/test?callback={CALLBACK}"),
        format!("/login/test?app=too-short&callback={CALLBACK}"),
        format!("/login/test?app={}&callback={CALLBACK}", "!".repeat(43)),
        format!("/login?app={challenge}&callback=https://evil.example.com/callback"),
    ];
    for path in cases {
        let (status, _, _) = request(&app, "GET", &path, "", &[], "").await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
    }
}

#[tokio::test]
async fn a_code_is_spent_by_its_first_try() {
    let app = browser_app(None, None).await;
    let (wrong_first, _) = sign_in(&app, "chris-subject", "chris").await;
    let (right_first, _) = sign_in(&app, "chris-subject", "chris").await;
    let (wrong_first, right_first) = (query(&wrong_first, "code"), query(&right_first, "code"));
    let statuses = [
        redeem(&app, &wrong_first, "another-verifier").await.0,
        redeem(&app, &wrong_first, VERIFIER).await.0,
        redeem(&app, &right_first, VERIFIER).await.0,
        redeem(&app, &right_first, VERIFIER).await.0,
        redeem(&app, "a-code-transom-never-made", VERIFIER).await.0,
    ];
    assert_eq!(
        statuses,
        [
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_REQUEST,
            StatusCode::OK,
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_REQUEST,
        ]
    );
}

/// The seconds Redis keeps `name` for.
async fn ttl(app: &TestApp, name: &str) -> i64 {
    let mut connection = app.app.store.connection().await.unwrap();
    connection.ttl(app.app.store.key(name)).await.unwrap()
}

#[tokio::test]
async fn a_code_lasts_a_minute() {
    let app = browser_app(None, None).await;
    let (location, _) = sign_in(&app, "chris-subject", "chris").await;
    let code = query(&location, "code");
    assert!((55..=60).contains(&ttl(&app, &format!("app-code:{}", hashed(&code))).await));
}

#[tokio::test]
async fn an_app_session_outlasts_a_browser_session() {
    let app = browser_app(None, None).await;
    let token = signed_in(&app).await;
    request(&app, "GET", "/@chris", &token, &[], "").await;
    let session = token.strip_prefix("transom_").unwrap();
    let days = ttl(&app, &format!("session:{}", hashed(session))).await / (24 * 60 * 60);
    assert!((364..=365).contains(&days), "{days}");
}

#[tokio::test]
async fn refuses_a_callback_taken_off_the_list_during_the_sign_in() {
    let app = browser_app(None, None).await;
    let (_, headers, _) = request(
        &app,
        "GET",
        &format!("/login/test?{}", app_query()),
        "",
        &[],
        "",
    )
    .await;
    let location = headers["location"].to_str().unwrap();
    let (state, nonce) = (query(location, "state"), query(location, "nonce"));
    let login = set_cookie(&headers, "__Host-transom-login").unwrap();
    let narrowed = App::new(
        crate::store::Store::new(
            redis::Client::open(crate::store::testing::redis_url()).unwrap(),
            app.app.store.key("").trim_end_matches(':').to_owned(),
        ),
        Issuers::of(vec![
            Issuer::new(app.issuer.browser_settings(None)).unwrap(),
        ]),
    )
    .calling_back_apps_at(vec![]);
    let narrowed = TestApp {
        app: Arc::new(narrowed),
        issuer: app.issuer,
    };
    let path = format!("/login/callback?code=s%7Cchris%7C{nonce}&state={state}");
    let cookie = format!("__Host-transom-login={login}");
    let (status, _, _) = request(&narrowed, "GET", &path, "", &[("cookie", &cookie)], "").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn signing_out_with_the_bearer_ends_the_session() {
    let app = browser_app(None, None).await;
    let token = signed_in(&app).await;
    let (signed_out, _, _) = request(&app, "POST", "/logout", &token, &[], "").await;
    let (after, _, _) = request(&app, "GET", "/@chris", &token, &[], "").await;
    assert_eq!(
        (signed_out, after),
        (StatusCode::NO_CONTENT, StatusCode::UNAUTHORIZED)
    );
}

#[tokio::test]
async fn challenges_a_session_token_transom_never_made() {
    let app = browser_app(None, None).await;
    let (status, headers, _) = request(&app, "GET", "/@chris", "transom_nope", &[], "").await;
    assert_eq!(
        (status, headers["www-authenticate"].to_str().unwrap()),
        (
            StatusCode::UNAUTHORIZED,
            "Bearer realm=\"transom\", error=\"invalid_token\""
        )
    );
}
