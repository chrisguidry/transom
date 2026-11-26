//! Refreshing a session with its issuer: requests that race to refresh
//! it, the issuer's answers that end it and those that do not, and the
//! code a client reads from the 401 of a session that is over.

use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use redis::AsyncCommands;
use serde_json::Value;

use super::browser_tests::{age, browser_app, session, sign_in};
use super::sessions::hashed;
use super::test_issuer::{Refreshing, TestIssuer};
use super::{Issuer, Issuers};
use crate::app::App;
use crate::app::testing::{TestApp, request};
use crate::metrics::testing::{sample, scrape};

/// A browser signed in as `@chris`, who has made a request, whose session
/// is due a refresh, and its cookie header.
async fn due(app: &TestApp) -> String {
    let cookie = sign_in(app, "chris-subject", "chris").await;
    status(app, &session(&cookie)).await;
    age(app, &cookie).await;
    session(&cookie)
}

/// The status of a request with the cookie header `cookie`.
async fn status(app: &TestApp, cookie: &str) -> StatusCode {
    request(app, "GET", "/@chris", "", &[("cookie", cookie)], "")
        .await
        .0
}

/// The count of refreshes through the test issuer with `outcome`.
async fn refreshes(app: &TestApp, outcome: &str) -> Option<String> {
    let series =
        format!("transom_session_refreshes_total{{outcome=\"{outcome}\",issuer=\"test\"}}");
    sample(&scrape(app).await, &series).map(str::to_owned)
}

#[tokio::test]
async fn requests_that_race_to_refresh_a_session_refresh_it_once() {
    let app = browser_app(None, None).await;
    let cookie = due(&app).await;
    let statuses = tokio::join!(
        status(&app, &cookie),
        status(&app, &cookie),
        status(&app, &cookie),
    );
    assert_eq!(statuses, (StatusCode::OK, StatusCode::OK, StatusCode::OK));
    assert_eq!(app.issuer.refreshes(), 1);
}

#[tokio::test]
async fn a_session_refreshes_again_with_the_token_its_issuer_rotated_to() {
    let app = browser_app(None, None).await;
    let cookie = sign_in(&app, "chris-subject", "chris").await;
    let header = session(&cookie);
    let mut statuses = Vec::new();
    for _ in 0..3 {
        age(&app, &cookie).await;
        statuses.push(status(&app, &header).await);
    }
    assert_eq!(statuses, [StatusCode::OK; 3]);
    assert_eq!(refreshes(&app, "refreshed").await.as_deref(), Some("3"));
}

#[tokio::test]
async fn only_invalid_grant_ends_a_session() {
    let cases = [
        (Refreshing::Refuses, StatusCode::UNAUTHORIZED, "ended"),
        (
            Refreshing::Errs(401, "invalid_grant"),
            StatusCode::UNAUTHORIZED,
            "ended",
        ),
        (
            Refreshing::Errs(401, "invalid_client"),
            StatusCode::OK,
            "refused",
        ),
        (
            Refreshing::Errs(400, "unsupported_grant_type"),
            StatusCode::OK,
            "refused",
        ),
        (
            Refreshing::Errs(400, "invalid_request"),
            StatusCode::OK,
            "refused",
        ),
        (Refreshing::Errs(404, ""), StatusCode::OK, "unavailable"),
        (Refreshing::Fails, StatusCode::OK, "unavailable"),
    ];
    for (refreshing, expected, outcome) in cases {
        let app = browser_app(None, None).await;
        let cookie = due(&app).await;
        app.issuer.refresh(refreshing);
        assert_eq!(status(&app, &cookie).await, expected, "{outcome}");
        assert_eq!(
            refreshes(&app, outcome).await.as_deref(),
            Some("1"),
            "{outcome}"
        );
    }
}

/// transom trusting the test issuer as a browser's, waiting at most 200
/// milliseconds for a refresh.
async fn hurried() -> TestApp {
    let issuer = TestIssuer::start().await;
    let trusted = Issuer::new(issuer.browser_settings(None))
        .unwrap()
        .refreshing_within(Duration::from_millis(200));
    let store = crate::store::testing::store();
    crate::identities::records::register_transom(&store)
        .await
        .unwrap();
    TestApp {
        app: Arc::new(App::new(store, Issuers::of(vec![trusted]))),
        issuer: Arc::new(issuer),
    }
}

#[tokio::test]
async fn a_session_whose_issuer_does_not_answer_in_time_carries_on() {
    let app = hurried().await;
    let cookie = due(&app).await;
    app.issuer.refresh(Refreshing::Hangs);
    assert_eq!(status(&app, &cookie).await, StatusCode::OK);
    assert_eq!(refreshes(&app, "unavailable").await.as_deref(), Some("1"));
}

#[tokio::test]
async fn a_token_the_issuer_rotated_too_late_for_the_request_is_still_kept() {
    let app = hurried().await;
    let cookie = sign_in(&app, "chris-subject", "chris").await;
    let header = session(&cookie);
    status(&app, &header).await;
    age(&app, &cookie).await;
    app.issuer
        .refresh(Refreshing::AcceptsAfter(Duration::from_millis(500)));
    let late = status(&app, &header).await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    app.issuer.refresh(Refreshing::Accepts);
    age(&app, &cookie).await;
    assert_eq!(
        (late, status(&app, &header).await),
        (StatusCode::OK, StatusCode::OK)
    );
    assert_eq!(refreshes(&app, "refreshed").await.as_deref(), Some("1"));
}

/// Replaces the refresh token of the session behind `cookie`, as another
/// server's refresh does.
async fn replace_refresh_token(app: &TestApp, cookie: &str) {
    let mut connection = app.app.store.connection().await.unwrap();
    let key = app.app.store.key(&format!("session:{}", hashed(cookie)));
    let () = connection.hset(key, "refresh", "newer").await.unwrap();
}

#[tokio::test]
async fn a_refusal_of_a_token_the_session_has_replaced_does_not_end_it() {
    let app = browser_app(None, None).await;
    let cookie = sign_in(&app, "chris-subject", "chris").await;
    age(&app, &cookie).await;
    app.issuer
        .refresh(Refreshing::RefusesAfter(Duration::from_millis(300)));
    let header = session(&cookie);
    let replacing = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        replace_refresh_token(&app, &cookie).await;
    };
    let (refused, ()) = tokio::join!(status(&app, &header), replacing);
    assert_eq!(
        (refused, status(&app, &header).await),
        (StatusCode::OK, StatusCode::OK)
    );
    assert_eq!(refreshes(&app, "refused").await.as_deref(), Some("1"));
}

/// The `code` of the body of the 401 that answers `cookie`.
async fn code(app: &TestApp, cookie: &str) -> Value {
    let (_, _, body) = request(app, "GET", "/@chris", "", &[("cookie", cookie)], "").await;
    serde_json::from_str::<Value>(&body).unwrap()["code"].clone()
}

#[tokio::test]
async fn a_session_that_is_over_says_why_in_its_code() {
    let app = browser_app(None, None).await;
    let signed_out = session(&sign_in(&app, "chris-subject", "chris").await);
    request(&app, "POST", "/logout", "", &[("cookie", &signed_out)], "").await;
    let ended = due(&app).await;
    app.issuer.refresh(Refreshing::Refuses);
    assert_eq!(
        (code(&app, &signed_out).await, code(&app, &ended).await),
        ("session_ended".into(), "issuer_ended".into())
    );
}

#[tokio::test]
async fn a_session_whose_issuer_cannot_be_discovered_carries_on() {
    let app = browser_app(None, None).await;
    let cookie = due(&app).await;
    let replica = app.replica();
    app.issuer.fail();
    assert_eq!(status(&replica, &cookie).await, StatusCode::OK);
    assert_eq!(
        refreshes(&replica, "unavailable").await.as_deref(),
        Some("1")
    );
}
