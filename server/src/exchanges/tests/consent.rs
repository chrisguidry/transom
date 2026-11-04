use axum::http::StatusCode;
use serde_json::Value;

use super::{JSON, decide, individual, send};
use crate::app::testing::{TestApp, events, request};

async fn get(app: &TestApp, token: &str, path: &str) -> (StatusCode, Value) {
    let (status, _, text) = request(app, "GET", path, token, &[], "").await;
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// Two individuals who have never exchanged anything.
async fn strangers() -> (TestApp, String, String) {
    let app = TestApp::new().await;
    let sender = individual(&app, "grocery-bot").await;
    let recipient = individual(&app, "chris").await;
    (app, sender, recipient)
}

#[tokio::test]
async fn a_strangers_first_message_is_held_while_transom_asks() {
    let (app, sender, recipient) = strangers().await;
    let (status, held) = send(&app, &sender, "/@chris", &[], "Milk is low").await;
    assert_eq!(
        (status, held["state"].as_str()),
        (StatusCode::CREATED, Some("pending"))
    );
    let (_, list) = get(&app, &recipient, "/@chris/exchanges").await;
    let request = &list["exchanges"][0];
    assert_eq!(request["from"], "@transom");
    assert_eq!(
        request["message"]["summary"],
        "@grocery-bot wants to send you messages"
    );
    let id = held["id"].as_str().unwrap();
    let (seen, _) = get(&app, &recipient, &format!("/exchanges/{id}")).await;
    let (early, _, _) = crate::app::testing::request(
        &app,
        "PUT",
        &format!("/exchanges/{id}/reply"),
        &recipient,
        &[],
        "ok",
    )
    .await;
    assert_eq!((seen, early), (StatusCode::OK, StatusCode::CONFLICT));
}

#[tokio::test]
async fn allowing_a_sender_releases_what_it_sent_and_what_it_sends() {
    let (app, sender, recipient) = strangers().await;
    let (_, first) = send(&app, &sender, "/@chris", &[], "Milk is low").await;
    let (_, second) = send(&app, &sender, "/@chris", &[], "Eggs too").await;
    let (_, list) = get(&app, &recipient, "/@chris/exchanges?state=open").await;
    assert_eq!(
        list["exchanges"].as_array().unwrap().len(),
        1,
        "one request for both"
    );
    decide(&app, &recipient, "chris", "allow").await;
    for held in [&first, &second] {
        let (_, exchange) = get(
            &app,
            &sender,
            &format!("/exchanges/{}", held["id"].as_str().unwrap()),
        )
        .await;
        assert_eq!(exchange["state"], "open");
    }
    let (_, third) = send(&app, &sender, "/@chris", &[], "Bread").await;
    assert_eq!(third["state"], "open");
}

#[tokio::test]
async fn declining_a_sender_declines_what_it_sent() {
    let (app, sender, recipient) = strangers().await;
    let (_, held) = send(&app, &sender, "/@chris", &[], "Milk is low").await;
    let id = held["id"].as_str().unwrap().to_owned();
    let deciding = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        decide(&app, &recipient, "chris", "decline").await;
    };
    let seen = events(&app, &format!("/exchanges/{id}"), &sender, 3, deciding).await;
    let states: Vec<_> = seen
        .iter()
        .map(|(_, exchange)| exchange["state"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(states, ["pending", "declined"]);
}

#[tokio::test]
async fn a_withdrawn_held_message_stays_withdrawn() {
    let (app, sender, recipient) = strangers().await;
    let (_, held) = send(&app, &sender, "/@chris", &[], "Never mind").await;
    let id = held["id"].as_str().unwrap();
    request(
        &app,
        "DELETE",
        &format!("/exchanges/{id}/message"),
        &sender,
        &[],
        "",
    )
    .await;
    decide(&app, &recipient, "chris", "allow").await;
    let (_, exchange) = get(&app, &sender, &format!("/exchanges/{id}")).await;
    assert_eq!(exchange["state"], "withdrawn");
}

#[tokio::test]
async fn a_consent_request_needs_a_choice() {
    let (app, sender, recipient) = strangers().await;
    send(&app, &sender, "/@chris", &[], "Milk is low").await;
    let (_, list) = get(&app, &recipient, "/@chris/exchanges").await;
    let id = list["exchanges"][0]["id"].as_str().unwrap();
    let (status, _, _) = request(
        &app,
        "PUT",
        &format!("/exchanges/{id}/reply"),
        &recipient,
        &[],
        "hmm",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sending_to_yourself_needs_no_consent() {
    let (app, _, recipient) = strangers().await;
    let (_, note) = send(&app, &recipient, "/@chris", &[], "Remember the milk").await;
    assert_eq!(note["state"], "open");
}

#[tokio::test]
async fn the_transom_name_is_always_taken() {
    let (app, sender, _) = strangers().await;
    let (claim, _, _) = request(&app, "PUT", "/@transom", &sender, &[], "").await;
    let (status, transom) = get(&app, &sender, "/@transom").await;
    assert_eq!((claim, status), (StatusCode::FORBIDDEN, StatusCode::OK));
    assert_eq!(transom["kind"], "individual");
}

/// `@chris` in a group `@household`, and `@spouse`, who is not in it.
async fn household() -> (TestApp, String, String) {
    let app = TestApp::new().await;
    let chris = individual(&app, "chris").await;
    let spouse = individual(&app, "spouse").await;
    request(&app, "PUT", "/@household", &chris, &[], "").await;
    (app, chris, spouse)
}

#[tokio::test]
async fn joining_a_group_takes_the_invitees_consent() {
    let (app, chris, spouse) = household().await;
    let (status, _, text) =
        request(&app, "PUT", "/@household/members/@spouse", &chris, &[], "").await;
    let (again, _, again_text) =
        request(&app, "PUT", "/@household/members/@spouse", &chris, &[], "").await;
    assert_eq!(
        (status, again),
        (StatusCode::ACCEPTED, StatusCode::ACCEPTED)
    );
    assert_eq!(text, again_text, "a second invitation reuses the first");
    let (before, _) = get(&app, &spouse, "/@household/members").await;
    decide(&app, &spouse, "spouse", "join").await;
    let (_, members) = get(&app, &spouse, "/@household/members").await;
    assert_eq!(before, StatusCode::FORBIDDEN);
    assert_eq!(members["members"], serde_json::json!(["@chris", "@spouse"]));
    let (member, _, _) = request(&app, "PUT", "/@household/members/@spouse", &chris, &[], "").await;
    assert_eq!(member, StatusCode::OK);
}

#[tokio::test]
async fn declining_an_invitation_leaves_things_as_they_were() {
    let (app, chris, spouse) = household().await;
    request(&app, "PUT", "/@household/members/@spouse", &chris, &[], "").await;
    decide(&app, &spouse, "spouse", "decline").await;
    let (_, members) = get(&app, &chris, "/@household/members").await;
    assert_eq!(members["members"], serde_json::json!(["@chris"]));
}

#[tokio::test]
async fn members_routes_refuse_what_they_cannot_do() {
    let (app, chris, spouse) = household().await;
    let cases = [
        (
            "PUT",
            "/@household/members/@spouse",
            &spouse,
            StatusCode::FORBIDDEN,
        ),
        ("GET", "/@chris/members", &chris, StatusCode::NOT_FOUND),
        (
            "PUT",
            "/@household/members/@household",
            &chris,
            StatusCode::BAD_REQUEST,
        ),
        (
            "PUT",
            "/@household/members/@nobody",
            &chris,
            StatusCode::NOT_FOUND,
        ),
        (
            "DELETE",
            "/@household/members/@spouse",
            &chris,
            StatusCode::NOT_FOUND,
        ),
    ];
    for (method, path, token, expected) in cases {
        let (status, _, _) = request(&app, method, path, token, &[], "").await;
        assert_eq!(status, expected, "{method} {path}");
    }
}

#[tokio::test]
async fn a_renamed_groups_members_redirect() {
    let (app, chris, spouse) = household().await;
    request(
        &app,
        "PATCH",
        "/@household",
        &chris,
        &[JSON],
        r#"{"name": "family"}"#,
    )
    .await;
    request(
        &app,
        "PATCH",
        "/@spouse",
        &spouse,
        &[JSON],
        r#"{"name": "partner"}"#,
    )
    .await;
    for (method, path) in [
        ("GET", "/@household/members"),
        ("PUT", "/@household/members/@spouse"),
        ("DELETE", "/@household/members/@chris"),
    ] {
        let (status, _, _) = request(&app, method, path, &chris, &[], "").await;
        assert_eq!(status, StatusCode::PERMANENT_REDIRECT, "{method} {path}");
    }
    let (moved, _, _) = request(&app, "PUT", "/@family/members/@spouse", &chris, &[], "").await;
    assert_eq!(moved, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn when_the_last_member_leaves_the_group_and_its_exchanges_are_gone() {
    let (app, chris, spouse) = household().await;
    let (_, sent) = send(&app, &spouse, "/@household", &[], "Dinner at 6").await;
    let id = sent["id"].as_str().unwrap().to_owned();
    let leaving = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let (status, _, _) = request(
            &app,
            "DELETE",
            "/@household/members/@chris",
            &chris,
            &[],
            "",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    };
    let seen = events(&app, &format!("/exchanges/{id}"), &spouse, 3, leaving).await;
    assert_eq!(seen.last().unwrap().0, "error");
    let (group, _) = get(&app, &chris, "/@household").await;
    let (exchange, _) = get(&app, &spouse, &format!("/exchanges/{id}")).await;
    let (_, list) = get(&app, &spouse, "/@spouse/exchanges").await;
    assert_eq!((group, exchange), (StatusCode::NOT_FOUND, StatusCode::GONE));
    assert_eq!(list["exchanges"].as_array().unwrap().len(), 0);
    let (reclaimed, _, _) = request(&app, "PUT", "/@household", &spouse, &[], "").await;
    assert_eq!(reclaimed, StatusCode::CREATED);
}

#[tokio::test]
async fn a_member_sends_to_its_own_group_without_asking() {
    let (app, chris, _) = household().await;
    let (_, sent) = send(&app, &chris, "/@household", &[], "Taking the trash out").await;
    assert_eq!(sent["state"], "open");
}

#[tokio::test]
async fn removing_a_member_leaves_the_group_to_the_rest() {
    let (app, chris, spouse) = household().await;
    request(&app, "PUT", "/@household/members/@spouse", &chris, &[], "").await;
    decide(&app, &spouse, "spouse", "join").await;
    let (status, _, _) = request(
        &app,
        "DELETE",
        "/@household/members/@spouse",
        &chris,
        &[],
        "",
    )
    .await;
    let (_, members) = get(&app, &chris, "/@household/members").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(members["members"], serde_json::json!(["@chris"]));
}

#[tokio::test]
async fn a_deleted_groups_former_names_are_freed() {
    let (app, chris, _) = household().await;
    request(
        &app,
        "PATCH",
        "/@household",
        &chris,
        &[JSON],
        r#"{"name": "family"}"#,
    )
    .await;
    request(&app, "DELETE", "/@family/members/@chris", &chris, &[], "").await;
    let (old, _) = get(&app, &chris, "/@household").await;
    assert_eq!(old, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_feed_announces_a_deleted_groups_exchanges_as_gone() {
    let (app, chris, spouse) = household().await;
    let (_, sent) = send(&app, &spouse, "/@household", &[], "Dinner at 6").await;
    decide(&app, &chris, "household", "allow").await;
    let id = sent["id"].as_str().unwrap().to_owned();
    let leaving = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        request(
            &app,
            "DELETE",
            "/@household/members/@chris",
            &chris,
            &[],
            "",
        )
        .await;
    };
    let seen = events(&app, "/@household/exchanges", &chris, 1, leaving).await;
    assert_eq!(
        (seen[0].0.as_str(), seen[0].1["id"].as_str()),
        ("gone", Some(id.as_str()))
    );
}
