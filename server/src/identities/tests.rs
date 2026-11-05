use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

use crate::app::testing::{TestApp, send};
use crate::store::testing::store_at;

fn body(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn location(headers: &HeaderMap) -> &str {
    headers.get("location").unwrap().to_str().unwrap()
}

/// Signs in an individual by claiming `name` with a fresh subject's token,
/// and returns that token.
async fn individual(app: &TestApp, name: &str) -> String {
    let token = app.issuer.token(&ulid::Ulid::generate().to_string());
    let (status, _, _) = send(app, "PUT", &format!("/@{name}"), Some(&token), None).await;
    assert_eq!(status, StatusCode::CREATED);
    token
}

#[tokio::test]
async fn a_new_token_claims_a_free_name() {
    let app = TestApp::new().await;
    let token = app.issuer.token("review-subject");
    let (status, headers, text) = send(&app, "PUT", "/@review-bot", Some(&token), None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(location(&headers), "/@review-bot");
    assert_eq!(
        body(&text),
        json!({ "name": "review-bot", "kind": "individual", "display_name": null, "picture": null })
    );
}

#[tokio::test]
async fn claiming_your_own_name_again_changes_nothing() {
    let app = TestApp::new().await;
    let token = individual(&app, "review-bot").await;
    let (status, _, text) = send(&app, "PUT", "/@review-bot", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body(&text)["kind"], "individual");
}

#[tokio::test]
async fn a_taken_name_is_forbidden_to_others() {
    let app = TestApp::new().await;
    individual(&app, "review-bot").await;
    let impostor = app.issuer.token("impostor");
    let (status, _, _) = send(&app, "PUT", "/@review-bot", Some(&impostor), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_individual_claiming_a_free_name_creates_a_group() {
    let app = TestApp::new().await;
    let token = individual(&app, "chris").await;
    let (status, _, text) = send(&app, "PUT", "/@household", Some(&token), None).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        body(&text),
        json!({ "name": "household", "kind": "group", "display_name": null, "picture": null })
    );
}

#[tokio::test]
async fn only_members_may_claim_a_group_again() {
    let app = TestApp::new().await;
    let member = individual(&app, "chris").await;
    send(&app, "PUT", "/@household", Some(&member), None).await;
    let outsider = individual(&app, "outsider").await;
    let (member_status, _, _) = send(&app, "PUT", "/@household", Some(&member), None).await;
    let (outsider_status, _, _) = send(&app, "PUT", "/@household", Some(&outsider), None).await;
    assert_eq!(
        (member_status, outsider_status),
        (StatusCode::OK, StatusCode::FORBIDDEN)
    );
}

#[tokio::test]
async fn a_first_request_signs_in_under_the_name_claim() {
    let app = TestApp::new().await;
    let token = app.issuer.token_named("chris-subject", "Chris");
    let (status, _, text) = send(&app, "GET", "/@chris", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body(&text),
        json!({ "name": "chris", "kind": "individual", "display_name": null, "picture": null })
    );
}

#[tokio::test]
async fn a_first_request_without_a_usable_name_conflicts() {
    let app = TestApp::new().await;
    let token = app.issuer.token_named("subject", "not a name");
    let (status, _, _) = send(&app, "GET", "/@anyone", Some(&token), None).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_first_request_whose_name_is_taken_conflicts() {
    let app = TestApp::new().await;
    individual(&app, "chris").await;
    let token = app.issuer.token_named("another-subject", "chris");
    let (status, _, _) = send(&app, "GET", "/@chris", Some(&token), None).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn an_unknown_name_is_not_found() {
    let app = TestApp::new().await;
    let token = individual(&app, "chris").await;
    let (status, _, _) = send(&app, "GET", "/@nobody", Some(&token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_path_without_an_at_is_not_found() {
    let app = TestApp::new().await;
    let token = individual(&app, "chris").await;
    let (status, _, _) = send(&app, "GET", "/chris", Some(&token), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_invalid_name_is_a_bad_request() {
    let app = TestApp::new().await;
    let token = app.issuer.token("subject");
    let (status, _, _) = send(&app, "PUT", "/@Review_Bot", Some(&token), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_rename_leaves_the_old_name_redirecting() {
    let app = TestApp::new().await;
    let token = individual(&app, "chris").await;
    let rename = json!({ "name": "guid.foo" });
    let (status, _, text) = send(&app, "PATCH", "/@chris", Some(&token), Some(rename)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body(&text)["name"], "guid.foo");
    let (status, headers, _) = send(&app, "GET", "/@chris", Some(&token), None).await;
    assert_eq!(
        (status, location(&headers)),
        (StatusCode::PERMANENT_REDIRECT, "/@guid.foo")
    );
}

#[tokio::test]
async fn claiming_your_own_former_name_redirects() {
    let app = TestApp::new().await;
    let token = individual(&app, "chris").await;
    send(
        &app,
        "PATCH",
        "/@chris",
        Some(&token),
        Some(json!({ "name": "guid.foo" })),
    )
    .await;
    let (status, headers, _) = send(&app, "PUT", "/@chris", Some(&token), None).await;
    assert_eq!(
        (status, location(&headers)),
        (StatusCode::PERMANENT_REDIRECT, "/@guid.foo")
    );
}

#[tokio::test]
async fn someone_else_may_claim_a_former_name() {
    let app = TestApp::new().await;
    let token = individual(&app, "chris").await;
    send(
        &app,
        "PATCH",
        "/@chris",
        Some(&token),
        Some(json!({ "name": "guid.foo" })),
    )
    .await;
    individual(&app, "chris").await;
    let (status, _, text) = send(&app, "GET", "/@chris", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body(&text)["name"], "chris");
}

#[tokio::test]
async fn renaming_a_former_name_redirects() {
    let app = TestApp::new().await;
    let token = individual(&app, "chris").await;
    send(
        &app,
        "PATCH",
        "/@chris",
        Some(&token),
        Some(json!({ "name": "guid.foo" })),
    )
    .await;
    let rename = json!({ "name": "someone" });
    let (status, _, _) = send(&app, "PATCH", "/@chris", Some(&token), Some(rename)).await;
    assert_eq!(status, StatusCode::PERMANENT_REDIRECT);
}

#[tokio::test]
async fn renaming_an_unknown_name_is_not_found() {
    let app = TestApp::new().await;
    let token = individual(&app, "chris").await;
    let rename = json!({ "name": "someone" });
    let (status, _, _) = send(&app, "PATCH", "/@nobody", Some(&token), Some(rename)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_an_individual_may_rename_itself() {
    let app = TestApp::new().await;
    individual(&app, "chris").await;
    let other = individual(&app, "other").await;
    let rename = json!({ "name": "stolen" });
    let (status, _, _) = send(&app, "PATCH", "/@chris", Some(&other), Some(rename)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_member_may_rename_its_group() {
    let app = TestApp::new().await;
    let token = individual(&app, "chris").await;
    send(&app, "PUT", "/@household", Some(&token), None).await;
    let rename = json!({ "name": "family" });
    let (status, _, text) = send(&app, "PATCH", "/@household", Some(&token), Some(rename)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body(&text),
        json!({ "name": "family", "kind": "group", "display_name": null, "picture": null })
    );
}

#[tokio::test]
async fn renaming_to_a_taken_name_conflicts() {
    let app = TestApp::new().await;
    let token = individual(&app, "chris").await;
    individual(&app, "taken").await;
    let rename = json!({ "name": "taken" });
    let (status, _, _) = send(&app, "PATCH", "/@chris", Some(&token), Some(rename)).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn renaming_to_an_invalid_name_is_a_bad_request() {
    let app = TestApp::new().await;
    let token = individual(&app, "chris").await;
    let rename = json!({ "name": "Not Valid" });
    let (status, _, _) = send(&app, "PATCH", "/@chris", Some(&token), Some(rename)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_display_name_is_set_and_removed() {
    let app = TestApp::new().await;
    let token = individual(&app, "grafana").await;
    let named = json!({ "display_name": "  Grafana Alerts " });
    let (status, _, text) = send(&app, "PATCH", "/@grafana", Some(&token), Some(named)).await;
    assert_eq!(
        (status, body(&text)["display_name"].clone()),
        (StatusCode::OK, json!("Grafana Alerts"))
    );
    let cleared = json!({ "display_name": "" });
    send(&app, "PATCH", "/@grafana", Some(&token), Some(cleared)).await;
    let (_, _, text) = send(&app, "GET", "/@grafana", Some(&token), None).await;
    assert_eq!(body(&text)["display_name"], Value::Null);
}

#[tokio::test]
async fn only_its_manager_may_change_a_display_name() {
    let app = TestApp::new().await;
    individual(&app, "grafana").await;
    let other = individual(&app, "other").await;
    let named = json!({ "display_name": "Not Grafana" });
    let (status, _, _) = send(&app, "PATCH", "/@grafana", Some(&other), Some(named)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_unusable_display_name_is_a_bad_request() {
    let app = TestApp::new().await;
    let token = individual(&app, "grafana").await;
    let named = json!({ "display_name": "Grafana\nAlerts" });
    let (status, _, text) = send(&app, "PATCH", "/@grafana", Some(&token), Some(named)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body(&text)["error"],
        "a display name is at most 64 characters, with no control characters"
    );
}

#[test]
fn a_display_name_is_short_and_printable() {
    let longest = "é".repeat(64);
    let too_long = "é".repeat(65);
    let cases = [
        ("Grafana", Ok(Some("Grafana"))),
        (" Grafana ", Ok(Some("Grafana"))),
        ("  ", Ok(None)),
        ("Zoë's ☕ bot", Ok(Some("Zoë's ☕ bot"))),
        (&longest, Ok(Some(longest.as_str()))),
        (&too_long, Err(())),
        ("tab\there", Err(())),
        ("bell\u{7}", Err(())),
    ];
    for (text, expected) in cases {
        let found = super::display_name(text);
        assert_eq!(
            found.as_ref().map(Option::as_deref).map_err(|_| ()),
            expected,
            "{text:?}"
        );
    }
}

#[tokio::test]
async fn is_unavailable_when_redis_is() {
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("redis://{}", closed.local_addr().unwrap());
    drop(closed);
    let app = TestApp::with_store(store_at(&url)).await;
    let token = app.issuer.token("subject");
    let (status, _, _) = send(&app, "GET", "/@chris", Some(&token), None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
