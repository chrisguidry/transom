use axum::http::StatusCode;
use serde_json::json;

use super::{Browser, JSON, Pair, devices, pair, subscribe};
use crate::app::testing::request;
use crate::signin::SESSION_PREFIX;
use crate::signin::sessions::{self, Kind, SignedIn};

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

pub(super) const PHONE: &str = r#"{"name": "Pixel 9", "wake": {"connection": true}}"#;

#[tokio::test]
async fn a_phone_that_holds_a_connection_is_a_device() {
    let pair = pair().await;
    let path = "/@chris/devices/phone";
    let (status, _, _) = request(&pair.app, "PUT", path, &pair.chris, &[JSON], PHONE).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        devices(&pair).await["devices"],
        json!([{ "id": "phone", "name": "Pixel 9", "wake": { "connection": true } }])
    );
}

#[tokio::test]
async fn a_connection_is_true() {
    let pair = pair().await;
    let device = json!({ "wake": { "connection": false } }).to_string();
    let path = "/@chris/devices/phone";
    let (status, _, _) = request(&pair.app, "PUT", path, &pair.chris, &[JSON], device).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

/// A session of the app's for a new person, `@pixel-owner`, as its bearer
/// token.
async fn app_session(pair: &Pair) -> String {
    let person = SignedIn {
        issuer: pair.app.issuer.url.clone(),
        subject: ulid::Ulid::generate().to_string(),
        name: "pixel-owner".to_owned(),
        refresh: String::new(),
    };
    let token = sessions::start(&pair.app.app, &person, Kind::App)
        .await
        .unwrap();
    let bearer = format!("{SESSION_PREFIX}{token}");
    request(&pair.app, "PUT", "/@pixel-owner", &bearer, &[], "").await;
    bearer
}

#[tokio::test]
async fn forgetting_a_phone_ends_the_session_that_registered_it() {
    let pair = pair().await;
    let phone = app_session(&pair).await;
    let path = "/@pixel-owner/devices/phone";
    let (registered, _, _) = request(&pair.app, "PUT", path, &phone, &[JSON], PHONE).await;
    let (forgotten, _, _) = request(&pair.app, "DELETE", path, &phone, &[], "").await;
    let (after, _, _) = request(&pair.app, "GET", "/@pixel-owner", &phone, &[], "").await;
    assert_eq!(
        [registered, forgotten, after],
        [
            StatusCode::CREATED,
            StatusCode::NO_CONTENT,
            StatusCode::UNAUTHORIZED
        ]
    );
}

#[tokio::test]
async fn a_device_cannot_name_a_session_to_end() {
    let pair = pair().await;
    let phone = app_session(&pair).await;
    let session = phone.strip_prefix(SESSION_PREFIX).unwrap();
    let mut device: serde_json::Value = serde_json::from_str(PHONE).unwrap();
    device["session"] = json!(sessions::hashed(session));
    let path = "/@chris/devices/phone";
    let device = device.to_string();
    request(&pair.app, "PUT", path, &pair.chris, &[JSON], device).await;
    request(&pair.app, "DELETE", path, &pair.chris, &[], "").await;
    let (after, _, _) = request(&pair.app, "GET", "/@pixel-owner", &phone, &[], "").await;
    assert_eq!(after, StatusCode::OK);
}

/// A registration token shaped like the ones FCM gave apps before
/// installation IDs, which a device may still register.
const TOKEN: &str =
    "cX9q2Lw3TQ-aB1cD_eF2gH:APA91bHk3Lm9-Zq_xYw2Rt7UvBnM0pQsTuVwXyZaBcDeFgHiJkLmNoPqRsTuVwXyZ";

/// The device a phone with FCM PUTs, waking by `token`.
pub(super) fn phone(token: &str) -> String {
    json!({ "name": "Pixel 9", "wake": { "fcm": token } }).to_string()
}

/// The status of registering `@chris`'s phone to wake by `token`.
async fn registering(token: &str) -> StatusCode {
    let pair = pair().await;
    let path = "/@chris/devices/phone";
    let device = phone(token);
    request(&pair.app, "PUT", path, &pair.chris, &[JSON], device)
        .await
        .0
}

#[tokio::test]
async fn a_phone_that_wakes_by_fcm_is_a_device() {
    let status = registering(TOKEN).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn a_phone_that_wakes_by_fcm_is_listed() {
    let pair = pair().await;
    let path = "/@chris/devices/phone";
    request(&pair.app, "PUT", path, &pair.chris, &[JSON], phone(TOKEN)).await;
    assert_eq!(
        devices(&pair).await["devices"],
        json!([{ "id": "phone", "name": "Pixel 9", "wake": { "fcm": TOKEN } }])
    );
}

#[tokio::test]
async fn an_fcm_token_is_short_and_plain() {
    let statuses = [
        registering("").await,
        registering(&"a".repeat(1024)).await,
        registering(&"a".repeat(1025)).await,
        registering("a token").await,
        registering("a/token").await,
    ];
    assert_eq!(
        statuses,
        [
            StatusCode::UNPROCESSABLE_ENTITY,
            StatusCode::CREATED,
            StatusCode::UNPROCESSABLE_ENTITY,
            StatusCode::UNPROCESSABLE_ENTITY,
            StatusCode::UNPROCESSABLE_ENTITY,
        ]
    );
}
