//! How the pages show who an exchange is from and to: a picture, or
//! initials in its place, and a display name with the handle beside it.

use crate::app::testing::{TestApp, decide, request};
use crate::identities::dimensions::testing::png;

const HTML: (&str, &str) = ("accept", "text/html,application/xhtml+xml");
const JSON: (&str, &str) = ("content-type", "application/json");

#[test]
fn initials_come_from_the_first_two_words() {
    let cases = [
        ("Grafana Alerts", "GA"),
        ("grafana", "G"),
        ("review-bot", "RB"),
        ("guid.foo", "GF"),
        ("one two three", "OT"),
        ("Élodie", "É"),
        ("", "?"),
    ];
    for (name, initials) in cases {
        assert_eq!(super::pages::initials(name), initials, "{name:?}");
    }
}

async fn individual(app: &TestApp, name: &str) -> String {
    let token = app.issuer.token(&ulid::Ulid::generate().to_string());
    request(app, "PUT", &format!("/@{name}"), &token, &[], "").await;
    token
}

/// `@grafana`, named Grafana and pictured, and `@chris`, with neither,
/// after `@grafana` sent `@chris` a question, and the question's id.
struct Sent {
    app: TestApp,
    grafana: String,
    chris: String,
    id: String,
}

async fn sent() -> Sent {
    sent_on(TestApp::new().await).await
}

async fn sent_on(app: TestApp) -> Sent {
    let grafana = individual(&app, "grafana").await;
    let chris = individual(&app, "chris").await;
    let named = r#"{"display_name": "Grafana"}"#;
    request(&app, "PATCH", "/@grafana", &grafana, &[JSON], named).await;
    request(&app, "PUT", "/@grafana/picture", &grafana, &[], png(64, 64)).await;
    let question = r#"{"summary": "Disk is filling", "choices": ["Ack"]}"#;
    let (_, _, sent) = request(&app, "POST", "/@chris", &grafana, &[JSON], question).await;
    let sent: serde_json::Value = serde_json::from_str(&sent).unwrap();
    decide(&app, &chris, "chris", "allow").await;
    let id = sent["id"].as_str().unwrap().to_owned();
    Sent {
        app,
        grafana,
        chris,
        id,
    }
}

#[tokio::test]
async fn an_inbox_row_shows_its_senders_picture_and_display_name() {
    let sent = sent().await;
    let (_, _, page) = request(
        &sent.app,
        "GET",
        "/@chris/exchanges",
        &sent.chris,
        &[HTML],
        "",
    )
    .await;
    for expected in [
        r#"<img class="avatar" src="/@grafana/picture?v="#,
        r#"<span title="@grafana">Grafana</span>"#,
    ] {
        assert!(page.contains(expected), "{expected} in {page}");
    }
}

#[tokio::test]
async fn an_inbox_row_shows_initials_for_who_has_no_picture() {
    let sent = sent().await;
    let (_, _, page) = request(
        &sent.app,
        "GET",
        "/@grafana/exchanges",
        &sent.grafana,
        &[HTML],
        "",
    )
    .await;
    let initials = r#"<span class="avatar initials" aria-hidden="true">C</span>"#;
    assert!(page.contains(initials), "{page}");
}

#[tokio::test]
async fn the_exchange_page_names_its_sender_by_display_name_and_handle() {
    let sent = sent().await;
    let path = format!("/exchanges/{}", sent.id);
    let (_, _, page) = request(&sent.app, "GET", &path, &sent.chris, &[HTML], "").await;
    for expected in [
        r#"<img class="avatar" src="/@grafana/picture?v="#,
        r#"Grafana <span class="handle">@grafana</span>"#,
    ] {
        assert!(page.contains(expected), "{expected} in {page}");
    }
}

#[tokio::test]
async fn a_page_loads_a_picture_from_its_own_origin() {
    let sent = sent_on(TestApp::reached_at("https://transom.example").await).await;
    let (_, _, page) = request(
        &sent.app,
        "GET",
        "/@chris/exchanges",
        &sent.chris,
        &[HTML],
        "",
    )
    .await;
    assert!(page.contains(r#"src="/@grafana/picture?v="#), "{page}");
}
