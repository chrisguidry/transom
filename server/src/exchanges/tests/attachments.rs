use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use super::{JSON, Pair, individual, pair, send};
use crate::app::testing::request;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n the rest of a png";
const JPEG: &[u8] = b"\xff\xd8\xff the rest of a jpeg";
const GIF: &[u8] = b"GIF87a the rest of a gif";
const WEBP: &[u8] = b"RIFF\0\0\0\0WEBP the rest of a webp";

fn image(name: &str, bytes: &[u8]) -> Value {
    json!({ "name": name, "data": STANDARD.encode(bytes) })
}

/// Sends `@chris` a message with `attachments` from `@review-bot`.
async fn send_images(pair: &Pair, body: &str, attachments: Value) -> (StatusCode, Value) {
    let message = json!({ "summary": "Look", "body": body, "attachments": attachments });
    send(
        &pair.app,
        &pair.sender,
        "/@chris",
        &[JSON],
        &message.to_string(),
    )
    .await
}

#[tokio::test]
async fn images_are_recorded_by_the_type_their_bytes_show() {
    let pair = pair().await;
    let images = json!([
        image("a.png", PNG),
        image("b.jpg", JPEG),
        image("c.gif", GIF),
        image("d.webp", WEBP)
    ]);
    let (status, exchange) = send_images(&pair, "", images).await;
    assert_eq!(status, StatusCode::CREATED);
    let types: Vec<&str> = exchange["message"]["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|attached| attached["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        types,
        ["image/png", "image/jpeg", "image/gif", "image/webp"]
    );
}

#[tokio::test]
async fn an_image_is_served_to_the_recipient_sandboxed() {
    let pair = pair().await;
    let (_, exchange) = send_images(&pair, "", json!([image("graph.gif", GIF)])).await;
    let id = exchange["id"].as_str().unwrap();
    let path = format!("/exchanges/{id}/attachments/graph.gif");
    assert_eq!(exchange["message"]["attachments"][0]["url"], path);
    let (status, headers, bytes) = request(&pair.app, "GET", &path, &pair.recipient, &[], "").await;
    assert_eq!(
        (
            status,
            headers["content-type"].to_str().unwrap(),
            headers["content-security-policy"].to_str().unwrap(),
        ),
        (StatusCode::OK, "image/gif", "default-src 'none'; sandbox")
    );
    assert_eq!(bytes.as_bytes(), GIF);
}

#[tokio::test]
async fn an_image_is_hidden_from_strangers() {
    let pair = pair().await;
    let (_, exchange) = send_images(&pair, "", json!([image("graph.png", PNG)])).await;
    let stranger = individual(&pair.app, "stranger").await;
    let path = format!(
        "/exchanges/{}/attachments/graph.png",
        exchange["id"].as_str().unwrap()
    );
    let (status, _, _) = request(&pair.app, "GET", &path, &stranger, &[], "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_image_the_message_lacks_is_not_found() {
    let pair = pair().await;
    let (_, exchange) = send_images(&pair, "", json!([image("graph.png", PNG)])).await;
    let path = format!(
        "/exchanges/{}/attachments/other.png",
        exchange["id"].as_str().unwrap()
    );
    let (status, _, _) = request(&pair.app, "GET", &path, &pair.recipient, &[], "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_image_whose_bytes_are_lost_is_not_found() {
    let pair = pair().await;
    let (_, exchange) = send_images(&pair, "", json!([image("graph.png", PNG)])).await;
    let id = exchange["id"].as_str().unwrap();
    let store = &pair.app.app.store;
    let mut connection = store.connection().await.unwrap();
    let () = redis::AsyncCommands::del(&mut connection, store.key(&format!("attachments:{id}")))
        .await
        .unwrap();
    let path = format!("/exchanges/{id}/attachments/graph.png");
    let (status, _, _) = request(&pair.app, "GET", &path, &pair.recipient, &[], "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn names_are_made_plain() {
    let pair = pair().await;
    let images = json!([
        image("../up/the graph.png", PNG),
        image(".hidden.gif", GIF),
        image("", JPEG)
    ]);
    let (_, exchange) = send_images(&pair, "", images).await;
    let names: Vec<&str> = exchange["message"]["attachments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|attached| attached["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["the-graph.png", "hidden.gif", "image"]);
}

#[tokio::test]
async fn only_images_are_attached() {
    let pair = pair().await;
    let (status, _) = send_images(&pair, "", json!([image("notes.txt", b"just text")])).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn two_images_cannot_share_a_name() {
    let pair = pair().await;
    let images = json!([image("graph.png", PNG), image("graph.png", GIF)]);
    let (status, _) = send_images(&pair, "", images).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_image_must_be_base64() {
    let pair = pair().await;
    let images = json!([{ "name": "graph.png", "data": "not base64!" }]);
    let (status, _) = send_images(&pair, "", images).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_message_carries_at_most_ten_images() {
    let pair = pair().await;
    let images: Vec<Value> = (0..11).map(|n| image(&format!("{n}.gif"), GIF)).collect();
    let (status, _) = send_images(&pair, "", json!(images)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_image_is_at_most_five_mebibytes() {
    let pair = pair().await;
    let large = [GIF, &vec![0; 5 * 1024 * 1024]].concat();
    let (status, _) = send_images(&pair, "", json!([image("large.gif", &large)])).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn a_messages_images_are_at_most_twenty_mebibytes_in_all() {
    let pair = pair().await;
    let large = [GIF, &vec![0; 4 * 1024 * 1024]].concat();
    let images: Vec<Value> = (0..5).map(|n| image(&format!("{n}.gif"), &large)).collect();
    let (status, _) = send_images(&pair, "", json!(images)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn the_page_places_named_images_in_the_body_and_the_rest_after_it() {
    let pair = pair().await;
    let images = json!([image("graph.png", PNG), image("log.gif", GIF)]);
    let (_, exchange) = send_images(&pair, "See ![the graph](graph.png).", images).await;
    let id = exchange["id"].as_str().unwrap();
    let html = [("accept", "text/html")];
    let (_, _, page) = request(
        &pair.app,
        "GET",
        &format!("/exchanges/{id}"),
        &pair.recipient,
        &html,
        "",
    )
    .await;
    let inline = format!(r#"<img src="/exchanges/{id}/attachments/graph.png" alt="the graph" />"#);
    let after = format!(r#"<img src="/exchanges/{id}/attachments/log.gif" alt="log.gif""#);
    assert!(page.contains(&inline) && page.contains(&after), "{page}");
}
