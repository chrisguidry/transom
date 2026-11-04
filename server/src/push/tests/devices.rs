use axum::http::StatusCode;
use serde_json::json;

use super::{Browser, JSON, devices, pair, subscribe};
use crate::app::testing::request;

#[tokio::test]
async fn a_browser_subscribes_resubscribes_and_unsubscribes() {
    let pair = pair().await;
    let browser = Browser::at("https://push.example.com/send/abc");
    let statuses = [
        subscribe(&pair, &browser).await,
        subscribe(&pair, &browser).await,
    ];
    assert_eq!(statuses, [StatusCode::CREATED, StatusCode::OK]);
    let listed = devices(&pair).await;
    assert_eq!(listed["key"].as_str().unwrap().len(), 87);
    assert_eq!(
        (
            listed["devices"][0]["id"].as_str(),
            listed["devices"][0]["name"].as_str(),
            listed["devices"][0]["wake"]["web_push"]["endpoint"].as_str(),
        ),
        (
            Some("laptop"),
            Some("Firefox on a laptop"),
            Some(browser.endpoint.as_str())
        )
    );
    let (status, _, _) = request(
        &pair.app,
        "DELETE",
        "/@chris/devices/laptop",
        &pair.chris,
        &[],
        "",
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(devices(&pair).await["devices"], json!([]));
}

#[tokio::test]
async fn only_the_identity_manages_its_devices() {
    let pair = pair().await;
    let (status, _, _) = request(&pair.app, "GET", "/@chris/devices", &pair.bot, &[], "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// `@chris`, renamed `@guid`, and the status and location of `method` on
/// his old name's devices.
async fn at_a_former_name(method: &str, path: &str) -> (StatusCode, String) {
    let pair = pair().await;
    let rename = json!({ "name": "guid" }).to_string();
    request(&pair.app, "PATCH", "/@chris", &pair.chris, &[JSON], rename).await;
    let browser = Browser::at("https://push.example.com/send/abc");
    let (status, headers, _) = request(
        &pair.app,
        method,
        path,
        &pair.chris,
        &[JSON],
        browser.device(),
    )
    .await;
    (status, headers["location"].to_str().unwrap().to_owned())
}

#[tokio::test]
async fn listing_at_a_former_name_redirects() {
    let moved = at_a_former_name("GET", "/@chris/devices").await;
    assert_eq!(moved, (StatusCode::PERMANENT_REDIRECT, "/@guid".to_owned()));
}

#[tokio::test]
async fn subscribing_at_a_former_name_redirects() {
    let moved = at_a_former_name("PUT", "/@chris/devices/laptop").await;
    assert_eq!(moved, (StatusCode::PERMANENT_REDIRECT, "/@guid".to_owned()));
}

#[tokio::test]
async fn unsubscribing_at_a_former_name_redirects() {
    let moved = at_a_former_name("DELETE", "/@chris/devices/laptop").await;
    assert_eq!(moved, (StatusCode::PERMANENT_REDIRECT, "/@guid".to_owned()));
}

#[tokio::test]
async fn a_damaged_push_key_is_an_error() {
    let pair = pair().await;
    let store = &pair.app.app.store;
    let mut connection = store.connection().await.unwrap();
    let () = redis::AsyncCommands::set(&mut connection, store.key("push-key"), "not a key")
        .await
        .unwrap();
    let (status, _, _) = request(&pair.app, "GET", "/@chris/devices", &pair.chris, &[], "").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

const POINT: &str =
    "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
const AUTH: &str = "BTBZMqHH6r4Tts7J_aSIgg";

/// The status of subscribing `@chris` as device `id`, at `endpoint`
/// with `p256dh`.
async fn subscribing(id: &str, endpoint: &str, p256dh: &str) -> StatusCode {
    let pair = pair().await;
    let subscription = json!({ "wake": { "web_push": {
        "endpoint": endpoint,
        "keys": { "p256dh": p256dh, "auth": AUTH },
    } } });
    request(
        &pair.app,
        "PUT",
        &format!("/@chris/devices/{id}"),
        &pair.chris,
        &[JSON],
        subscription.to_string(),
    )
    .await
    .0
}

#[tokio::test]
async fn a_device_needs_a_web_endpoint() {
    let status = subscribing("laptop", "ftp://push.example.com/abc", POINT).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_device_needs_a_whole_key() {
    let status = subscribing("laptop", "https://push.example.com/abc", "BCVx").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_device_needs_a_key_on_the_curve() {
    let not_a_point = format!("BA{}", "A".repeat(85));
    let status = subscribing("laptop", "https://push.example.com/abc", &not_a_point).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_device_id_is_short_and_plain() {
    let status = subscribing("lap.top", "https://push.example.com/abc", POINT).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}
