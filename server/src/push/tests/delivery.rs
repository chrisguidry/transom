use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED, UnparsedPublicKey};
use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use redis::AsyncCommands;
use serde_json::{Value, json};

use super::{Browser, JSON, Pair, PushService, devices, pair, subscribe};
use crate::app::testing::request;
use crate::exchanges::records::now;
use crate::push::deliver_due;

const MINUTE: i64 = 60 * 1000;

/// `@chris`, subscribed at a push service.
async fn subscribed() -> (Pair, PushService, Browser) {
    let pair = pair().await;
    let service = PushService::start().await;
    let browser = Browser::at(&service.url);
    subscribe(&pair, &browser).await;
    (pair, service, browser)
}

/// Sends `@chris` a question from `@review-bot`, and returns its id.
async fn ask(pair: &Pair) -> String {
    let body = json!({
        "summary": "Deploy?",
        "body": "The build passed.",
        "choices": ["Ship it", { "label": "Wait", "id": "wait" }, "Never"],
        "urgency": "high",
        "ttl": 600,
    });
    let (_, _, text) = request(
        &pair.app,
        "POST",
        "/@chris",
        &pair.bot,
        &[JSON],
        body.to_string(),
    )
    .await;
    serde_json::from_str::<Value>(&text).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn an_arrival_reaches_the_browser_encrypted() {
    let (pair, service, browser) = subscribed().await;
    let id = ask(&pair).await;
    assert_eq!(deliver_due(&pair.app.app, now()).await.unwrap(), 1);
    let (headers, body) = service.received().remove(0);
    let header = |name: &str| headers[name].to_str().unwrap().to_owned();
    assert_eq!(
        (
            header("content-encoding"),
            header("urgency"),
            header("topic")
        ),
        ("aes128gcm".to_owned(), "high".to_owned(), id.clone())
    );
    assert!((590..=600).contains(&header("ttl").parse::<i64>().unwrap()));
    assert_eq!(
        browser.read(&body),
        json!({
            "event": "received",
            "id": id,
            "url": format!("/exchanges/{id}"),
            "title": "Deploy?",
            "body": "@review-bot: The build passed.",
            "choices": [
                { "label": "Ship it", "key": "Ship it" },
                { "label": "Wait", "key": "wait" },
            ],
        })
    );
}

#[tokio::test]
async fn a_long_summary_is_cut_to_fit_a_notification() {
    let (pair, service, browser) = subscribed().await;
    let summary = "word ".repeat(100);
    let body = json!({ "summary": summary }).to_string();
    request(&pair.app, "POST", "/@chris", &pair.bot, &[JSON], body).await;
    deliver_due(&pair.app.app, now()).await.unwrap();
    let (_, sent) = service.received().remove(0);
    let title = browser.read(&sent)["title"].as_str().unwrap().to_owned();
    assert_eq!((title.chars().count(), title.ends_with('…')), (120, true));
}

#[tokio::test]
async fn each_push_carries_a_vapid_token_for_its_push_service() {
    let (pair, service, _) = subscribed().await;
    ask(&pair).await;
    deliver_due(&pair.app.app, now()).await.unwrap();
    let (headers, _) = service.received().remove(0);
    let authorization = headers["authorization"].to_str().unwrap();
    let (token, key) = authorization
        .strip_prefix("vapid t=")
        .unwrap()
        .split_once(", k=")
        .unwrap();
    assert_eq!(key, devices(&pair).await["key"]);
    let (input, signature) = token.rsplit_once('.').unwrap();
    let public = URL_SAFE_NO_PAD.decode(key).unwrap();
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, public)
        .verify(
            input.as_bytes(),
            &URL_SAFE_NO_PAD.decode(signature).unwrap(),
        )
        .unwrap();
    let claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(input.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap();
    let origin = service.url.trim_end_matches("/push/abc");
    assert_eq!(
        (claims["aud"].as_str(), claims["sub"].as_str()),
        (Some(origin), Some("https://transom.sh"))
    );
    assert!(claims["exp"].as_i64().unwrap() > now() / 1000);
}

#[tokio::test]
async fn an_ending_clears_the_notification() {
    let (pair, service, browser) = subscribed().await;
    let id = ask(&pair).await;
    let reply = format!("/exchanges/{id}/reply");
    request(
        &pair.app,
        "PUT",
        &reply,
        &pair.chris,
        &[JSON],
        r#"{"choice": "wait"}"#,
    )
    .await;
    deliver_due(&pair.app.app, now()).await.unwrap();
    let pushes: Vec<Value> = service
        .received()
        .iter()
        .map(|(_, body)| browser.read(body))
        .collect();
    assert_eq!(pushes[1], json!({ "event": "ended", "id": id }));
}

#[tokio::test]
async fn a_device_its_push_service_forgot_is_forgotten() {
    let (pair, service, _) = subscribed().await;
    service.answer(StatusCode::GONE);
    ask(&pair).await;
    deliver_due(&pair.app.app, now()).await.unwrap();
    assert_eq!(devices(&pair).await["devices"], json!([]));
}

#[tokio::test]
async fn a_busy_push_service_is_tried_again() {
    let (pair, service, _) = subscribed().await;
    service.answer(StatusCode::SERVICE_UNAVAILABLE);
    ask(&pair).await;
    let app = &pair.app.app;
    let attempted = [
        deliver_due(app, now()).await.unwrap(),
        deliver_due(app, now()).await.unwrap(),
        deliver_due(app, now() + MINUTE).await.unwrap(),
    ];
    assert_eq!(attempted, [1, 0, 1]);
}

#[tokio::test]
async fn an_unreachable_push_service_is_tried_again() {
    let pair = pair().await;
    subscribe(&pair, &Browser::at("http://127.0.0.1:1/push")).await;
    ask(&pair).await;
    let app = &pair.app.app;
    let attempted = [
        deliver_due(app, now()).await.unwrap(),
        deliver_due(app, now() + MINUTE).await.unwrap(),
    ];
    assert_eq!(attempted, [1, 1]);
}

#[tokio::test]
async fn a_push_is_dropped_after_an_hour_of_trying() {
    let (pair, service, _) = subscribed().await;
    service.answer(StatusCode::SERVICE_UNAVAILABLE);
    ask(&pair).await;
    let app = &pair.app.app;
    let attempted = [
        deliver_due(app, now() + 61 * MINUTE).await.unwrap(),
        deliver_due(app, now() + 120 * MINUTE).await.unwrap(),
    ];
    assert_eq!(attempted, [1, 0]);
}

#[tokio::test]
async fn a_refused_push_is_dropped() {
    let (pair, service, _) = subscribed().await;
    service.answer(StatusCode::BAD_REQUEST);
    ask(&pair).await;
    let app = &pair.app.app;
    let attempted = [
        deliver_due(app, now()).await.unwrap(),
        deliver_due(app, now() + MINUTE).await.unwrap(),
    ];
    assert_eq!(attempted, [1, 0]);
}

#[tokio::test]
async fn a_removed_device_gets_nothing_more() {
    let (pair, service, _) = subscribed().await;
    ask(&pair).await;
    request(
        &pair.app,
        "DELETE",
        "/@chris/devices/laptop",
        &pair.chris,
        &[],
        "",
    )
    .await;
    deliver_due(&pair.app.app, now()).await.unwrap();
    assert_eq!(service.received().len(), 0);
}

#[tokio::test]
async fn a_push_another_server_claimed_is_skipped() {
    let pair = pair().await;
    let store = &pair.app.app.store;
    let mut connection = store.connection().await.unwrap();
    let () = connection
        .zadd(store.key("pushes"), "claimed-elsewhere", 0)
        .await
        .unwrap();
    assert_eq!(deliver_due(&pair.app.app, now()).await.unwrap(), 0);
}
