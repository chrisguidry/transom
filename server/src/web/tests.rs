//! The pages, as a browser asks for them.

use axum::http::StatusCode;

use crate::app::testing::{TestApp, decide, request};

const HTML: (&str, &str) = ("accept", "text/html,application/xhtml+xml");
const JSON: (&str, &str) = ("content-type", "application/json");

async fn individual(app: &TestApp, name: &str) -> String {
    let token = app.issuer.token(&ulid::Ulid::generate().to_string());
    request(app, "PUT", &format!("/@{name}"), &token, &[], "").await;
    token
}

/// `@chris`, and an exchange from `@review-bot` that `@chris` allowed.
async fn with_a_question(body: &str) -> (TestApp, String, String, String) {
    let app = TestApp::new().await;
    let bot = individual(&app, "review-bot").await;
    let chris = individual(&app, "chris").await;
    let (_, _, sent) = request(&app, "POST", "/@chris", &bot, &[JSON], body.to_owned()).await;
    let sent: serde_json::Value = serde_json::from_str(&sent).unwrap();
    decide(&app, &chris, "chris", "allow").await;
    (app, bot, chris, sent["id"].as_str().unwrap().to_owned())
}

#[tokio::test]
async fn home_is_the_inbox() {
    let app = TestApp::new().await;
    let chris = individual(&app, "chris").await;
    let (status, headers, _) = request(&app, "GET", "/", &chris, &[], "").await;
    assert_eq!(
        (status, headers["location"].to_str().unwrap()),
        (StatusCode::SEE_OTHER, "/@chris/exchanges")
    );
}

#[tokio::test]
async fn a_browser_that_is_not_signed_in_is_sent_to_sign_in() {
    let app = TestApp::new().await;
    let (status, headers, _) = request(&app, "GET", "/@chris/exchanges", "", &[HTML], "").await;
    assert_eq!(
        (status, headers["location"].to_str().unwrap()),
        (StatusCode::SEE_OTHER, "/login?next=%2F%40chris%2Fexchanges")
    );
    let (api, _, _) = request(&app, "GET", "/@chris/exchanges", "", &[], "").await;
    assert_eq!(api, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_sign_in_page_offers_each_browser_issuer() {
    let app = TestApp::new().await;
    let (status, headers, page) =
        request(&app, "GET", "/login?next=/@chris", "", &[HTML], "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("No issuer is set up"), "{page}");
    assert!(
        headers["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("script-src 'self'")
    );
}

#[tokio::test]
async fn the_inbox_puts_what_waits_on_you_first() {
    let body = r#"{"summary": "Restart postgres?", "choices": ["Restart", "Leave it"]}"#;
    let (app, _, chris, _) = with_a_question(body).await;
    let (status, _, page) = request(&app, "GET", "/@chris/exchanges", &chris, &[HTML], "").await;
    assert_eq!(status, StatusCode::OK);
    let waiting = page.find("Waiting on you").unwrap();
    let question = page.find("Restart postgres?").unwrap();
    let everything = page.find("Everything else").unwrap();
    assert!(waiting < question && question < everything, "{page}");
}

#[tokio::test]
async fn an_empty_inbox_says_so() {
    let app = TestApp::new().await;
    let chris = individual(&app, "chris").await;
    let (_, _, page) = request(&app, "GET", "/@chris/exchanges", &chris, &[HTML], "").await;
    assert!(page.contains("Nothing here yet"), "{page}");
}

#[tokio::test]
async fn a_question_page_offers_its_choices_and_a_text_box() {
    let body = r#"{"summary": "Restart postgres?", "body": "The disk is at **94%**.",
        "choices": [{"label": "Restart", "id": "restart", "description": "About *30 seconds* down."}, "Leave it"],
        "prompt": "Why not?", "link": [{"href": "https://grafana/d", "rel": "related"}]}"#;
    let (app, _, chris, id) = with_a_question(body).await;
    let (status, _, page) = request(
        &app,
        "GET",
        &format!("/exchanges/{id}"),
        &chris,
        &[HTML],
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for expected in [
        "<strong>94%</strong>",
        r#"name="choice" value="restart">Restart</button>"#,
        "<em>30 seconds</em>",
        "Why not?",
        "related: https://grafana/d",
        "hx-put=\"/exchanges/",
    ] {
        assert!(page.contains(expected), "{expected} in {page}");
    }
}

#[tokio::test]
async fn reply_sends_text_alone_and_stays_quiet_beside_choices() {
    for (body, reply) in [
        (
            r#"{"summary": "Restart postgres?", "choices": ["Restart"]}"#,
            r#"<button type="submit" class="quiet" data-needs-text disabled>Reply</button>"#,
        ),
        (
            r#"{"summary": "Why did it fail?"}"#,
            r#"<button type="submit" class="primary" data-needs-text disabled>Reply</button>"#,
        ),
    ] {
        let (app, _, chris, id) = with_a_question(body).await;
        let (_, _, page) = request(
            &app,
            "GET",
            &format!("/exchanges/{id}"),
            &chris,
            &[HTML],
            "",
        )
        .await;
        assert!(page.contains(reply), "{reply} in {page}");
    }
}

#[tokio::test]
async fn a_question_with_choices_takes_text_with_no_choice() {
    let body = r#"{"summary": "Restart postgres?", "choices": ["Restart", "Leave it"]}"#;
    let (app, bot, chris, id) = with_a_question(body).await;
    let form = ("content-type", "application/x-www-form-urlencoded");
    let (status, _, _) = request(
        &app,
        "PUT",
        &format!("/exchanges/{id}/reply"),
        &chris,
        &[form, ("hx-request", "true")],
        "text=Not+until+the+backup+finishes",
    )
    .await;
    assert!(status.is_success(), "{status}");
    let (_, _, page) = request(&app, "GET", &format!("/exchanges/{id}"), &bot, &[HTML], "").await;
    assert!(
        page.contains("<p>Not until the backup finishes</p>"),
        "{page}"
    );
    assert!(!page.contains(r#"class="chosen""#), "{page}");
}

#[tokio::test]
async fn a_replied_question_shows_the_reply_by_its_choices_label() {
    let body = r#"{"summary": "Restart postgres?", "choices": [{"label": "Restart now", "id": "restart"}]}"#;
    let (app, bot, chris, id) = with_a_question(body).await;
    request(
        &app,
        "PUT",
        &format!("/exchanges/{id}/reply"),
        &chris,
        &[JSON],
        r#"{"choice": "restart", "text": "after 5pm"}"#,
    )
    .await;
    let (_, _, page) = request(&app, "GET", &format!("/exchanges/{id}"), &bot, &[HTML], "").await;
    assert!(page.contains("@chris replied"), "{page}");
    assert!(
        page.contains(r#"<p class="chosen">Restart now</p>"#),
        "{page}"
    );
    assert!(page.contains("after 5pm"), "{page}");
    assert!(!page.contains("Withdraw"), "{page}");
}

#[tokio::test]
async fn the_sender_can_withdraw_an_open_question() {
    let body = r#"{"body": "A very long first line that keeps going well past the eighty characters a title may have before it is cut"}"#;
    let (app, bot, _, id) = with_a_question(body).await;
    let (_, _, page) = request(&app, "GET", &format!("/exchanges/{id}"), &bot, &[HTML], "").await;
    assert!(page.contains("Withdraw"), "{page}");
    assert!(page.contains("eighty characters a title …"), "{page}");
}

#[tokio::test]
async fn a_groups_members_page_lists_them_and_invites() {
    let app = TestApp::new().await;
    let chris = individual(&app, "chris").await;
    request(&app, "PUT", "/@household", &chris, &[], "").await;
    let (status, _, page) = request(&app, "GET", "/@household/members", &chris, &[HTML], "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains("@chris") && page.contains("data-invite=\"/@household/members/@\""),
        "{page}"
    );
}

#[tokio::test]
async fn serves_its_static_files() {
    let app = TestApp::new().await;
    for (file, content_type) in [
        ("htmx-2.0.11.min.js", "text/javascript"),
        ("htmx-ext-sse-2.2.4.min.js", "text/javascript"),
        ("transom.js", "text/javascript"),
        ("transom.css", "text/css"),
        ("transom.svg", "image/svg+xml"),
        ("transom.webmanifest", "application/manifest+json"),
    ] {
        let (status, headers, _) =
            request(&app, "GET", &format!("/static/{file}"), "", &[], "").await;
        assert_eq!(
            (status, headers["content-type"].to_str().unwrap()),
            (StatusCode::OK, content_type),
            "{file}"
        );
    }
    let (missing, _, _) = request(&app, "GET", "/static/nothing.js", "", &[], "").await;
    assert_eq!(missing, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn serves_its_service_worker_from_the_root() {
    let app = TestApp::new().await;
    let (status, headers, script) = request(&app, "GET", "/sw.js", "", &[], "").await;
    assert_eq!(
        (status, headers["content-type"].to_str().unwrap()),
        (StatusCode::OK, "text/javascript")
    );
    assert!(script.contains("showNotification"));
}

#[tokio::test]
async fn the_bar_offers_notifications_for_the_person_reading() {
    let (app, _, chris, _) = with_a_question(r#"{"summary": "hello"}"#).await;
    let (_, _, page) = request(&app, "GET", "/@chris/exchanges", &chris, &[HTML], "").await;
    assert!(page.contains(r#"data-devices="/@chris/devices""#), "{page}");
}

#[tokio::test]
async fn htmx_gets_html_without_asking_for_it_by_accept() {
    let body = r#"{"summary": "Restart postgres?", "choices": ["Restart"]}"#;
    let (app, _, chris, id) = with_a_question(body).await;
    let (_, _, part) = request(
        &app,
        "GET",
        &format!("/exchanges/{id}"),
        &chris,
        &[("hx-request", "true")],
        "",
    )
    .await;
    assert!(part.contains(r#"<article id="exchange""#), "{part}");
}
