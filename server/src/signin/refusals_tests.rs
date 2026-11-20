//! What `/metrics` counts of the sign-ins transom refuses.

use super::browser::SESSION_COOKIE;
use super::test_issuer::TestIssuer;
use crate::app::testing::{TestApp, request};
use crate::metrics::testing::{sample, scrape};

/// The count of refusals for `reason` through `issuer` on `app`'s
/// `/metrics`.
async fn refusals(app: &TestApp, reason: &str, issuer: &str) -> Option<String> {
    let text = scrape(app).await;
    let series =
        format!("transom_signin_failures_total{{reason=\"{reason}\",issuer=\"{issuer}\"}}");
    sample(&text, &series).map(str::to_owned)
}

#[tokio::test]
async fn each_refusal_is_counted_by_its_reason() {
    let app = TestApp::new().await;
    let stranger = TestIssuer::start().await;
    let cookie = format!("{SESSION_COOKIE}=whatever");
    for (method, token, headers, reason, issuer) in [
        ("GET", String::new(), vec![], "no_credentials", "none"),
        ("GET", "not-a-jwt".to_owned(), vec![], "malformed", "none"),
        (
            "GET",
            stranger.token("u-1"),
            vec![],
            "untrusted_issuer",
            "none",
        ),
        (
            "GET",
            app.issuer.token_signed_by_stranger("u-1"),
            vec![],
            "invalid",
            "test",
        ),
        (
            "GET",
            "transom_ended".to_owned(),
            vec![],
            "session_ended",
            "none",
        ),
        (
            "PUT",
            String::new(),
            vec![("cookie", cookie.as_str())],
            "cross_site",
            "none",
        ),
    ] {
        request(&app, method, "/@chris", &token, &headers, "").await;
        assert_eq!(
            refusals(&app, reason, issuer).await.as_deref(),
            Some("1"),
            "{reason}"
        );
    }
}

#[tokio::test]
async fn an_issuer_whose_keys_cannot_be_fetched_is_counted() {
    let app = TestApp::new().await;
    app.issuer.fail();
    let token = app.issuer.token("u-1");
    request(&app, "GET", "/@chris", &token, &[], "").await;
    assert_eq!(
        refusals(&app, "issuer_unavailable", "test")
            .await
            .as_deref(),
        Some("1")
    );
}
