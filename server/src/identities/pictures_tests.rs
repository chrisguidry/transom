//! An identity's picture, at `/@{name}/picture`.

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::dimensions::testing::png;
use crate::app::testing::{TestApp, request};

/// Sends a request with a binary body as `token`, and returns the status,
/// the headers, and the body's bytes, which a picture's are.
async fn fetch(
    app: &TestApp,
    method: &str,
    path: &str,
    token: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(path);
    if !token.is_empty() {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = crate::app::router(app.app.clone())
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, bytes.to_vec())
}

async fn individual(app: &TestApp, name: &str) -> String {
    let token = app.issuer.token(&ulid::Ulid::generate().to_string());
    request(app, "PUT", &format!("/@{name}"), &token, &[], "").await;
    token
}

#[tokio::test]
async fn transoms_own_identity_has_a_picture() {
    let app = TestApp::new().await;
    let chris = individual(&app, "chris").await;
    let (status, headers, bytes) =
        fetch(&app, "GET", "/@transom/picture", &chris, &[], vec![]).await;
    assert_eq!(
        (
            status,
            headers["content-type"].to_str().unwrap(),
            &bytes[1..4]
        ),
        (StatusCode::OK, "image/png", b"PNG".as_slice())
    );
}

/// Puts `bytes` as `@grafana`'s picture, as `@grafana`.
async fn putting(bytes: Vec<u8>) -> (StatusCode, Value) {
    let app = TestApp::new().await;
    let grafana = individual(&app, "grafana").await;
    let (status, _, body) = fetch(&app, "PUT", "/@grafana/picture", &grafana, &[], bytes).await;
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn a_picture_is_described_with_its_identity() {
    let (status, body) = putting(png(64, 64)).await;
    assert_eq!(status, StatusCode::OK);
    let picture = body["picture"].as_str().unwrap();
    assert!(picture.starts_with("/@grafana/picture?v="), "{picture}");
}

#[tokio::test]
async fn a_picture_that_is_not_png_jpeg_or_webp_is_unsupported() {
    let (status, body) = putting(b"GIF89a\x40\0\x40\0".to_vec()).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(body["error"], "a picture is a PNG, JPEG, or WebP image");
}

#[tokio::test]
async fn an_svg_picture_is_unsupported() {
    let svg = b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec();
    assert_eq!(putting(svg).await.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn a_picture_over_256_kib_is_too_large() {
    let mut bytes = png(64, 64);
    bytes.resize(256 * 1024 + 1, 0);
    let (status, body) = putting(bytes).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body["error"], "a picture is at most 262144 bytes");
}

#[tokio::test]
async fn a_picture_that_is_not_square_is_refused() {
    let (status, body) = putting(png(64, 48)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body["error"],
        "a picture is square, and this one is 64 by 48 pixels"
    );
}

#[tokio::test]
async fn a_picture_over_1024_pixels_a_side_is_refused() {
    let (status, body) = putting(png(1025, 1025)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body["error"],
        "a picture is at most 1024 pixels a side, and this one is 1025"
    );
}

#[tokio::test]
async fn a_picture_whose_size_cannot_be_read_is_refused() {
    let (status, body) = putting(png(64, 64)[..18].to_vec()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "the picture's size cannot be read");
}

/// `@grafana`, signed in, with a picture, and `@chris`, who is signed in.
struct Pictured {
    app: TestApp,
    grafana: String,
    chris: String,
}

async fn pictured() -> Pictured {
    let app = TestApp::new().await;
    let grafana = individual(&app, "grafana").await;
    let chris = individual(&app, "chris").await;
    fetch(&app, "PUT", "/@grafana/picture", &grafana, &[], png(64, 64)).await;
    Pictured {
        app,
        grafana,
        chris,
    }
}

#[tokio::test]
async fn anyone_signed_in_sees_a_picture_with_its_type_and_validator() {
    let pictured = pictured().await;
    let (status, headers, bytes) = fetch(
        &pictured.app,
        "GET",
        "/@grafana/picture",
        &pictured.chris,
        &[],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, png(64, 64));
    assert_eq!(headers["content-type"], "image/png");
    assert_eq!(headers["cache-control"], "private, no-cache");
    let (_, _, described) =
        request(&pictured.app, "GET", "/@grafana", &pictured.chris, &[], "").await;
    let described: Value = serde_json::from_str(&described).unwrap();
    let version = described["picture"]
        .as_str()
        .unwrap()
        .split("?v=")
        .nth(1)
        .unwrap()
        .to_owned();
    assert_eq!(headers["etag"].to_str().unwrap(), format!("\"{version}\""));
}

#[tokio::test]
async fn a_picture_the_caller_has_is_not_modified() {
    let pictured = pictured().await;
    let (_, headers, _) = fetch(
        &pictured.app,
        "GET",
        "/@grafana/picture",
        &pictured.chris,
        &[],
        vec![],
    )
    .await;
    let etag = headers["etag"].to_str().unwrap();
    let (status, _, bytes) = fetch(
        &pictured.app,
        "GET",
        "/@grafana/picture",
        &pictured.chris,
        &[("if-none-match", etag)],
        vec![],
    )
    .await;
    assert_eq!((status, bytes.len()), (StatusCode::NOT_MODIFIED, 0));
}

#[tokio::test]
async fn a_picture_needs_a_signed_in_caller() {
    let pictured = pictured().await;
    let (status, _, _) = fetch(&pictured.app, "GET", "/@grafana/picture", "", &[], vec![]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_new_picture_has_a_new_url() {
    let pictured = pictured().await;
    let (_, _, before) = request(&pictured.app, "GET", "/@grafana", &pictured.chris, &[], "").await;
    let (_, _, after) = fetch(
        &pictured.app,
        "PUT",
        "/@grafana/picture",
        &pictured.grafana,
        &[],
        png(32, 32),
    )
    .await;
    let before: Value = serde_json::from_str(&before).unwrap();
    let after: Value = serde_json::from_slice(&after).unwrap();
    assert_ne!(before["picture"], after["picture"]);
}

#[tokio::test]
async fn a_removed_picture_is_gone() {
    let pictured = pictured().await;
    let (status, _, _) = fetch(
        &pictured.app,
        "DELETE",
        "/@grafana/picture",
        &pictured.grafana,
        &[],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = fetch(
        &pictured.app,
        "GET",
        "/@grafana/picture",
        &pictured.chris,
        &[],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, _, described) =
        request(&pictured.app, "GET", "/@grafana", &pictured.chris, &[], "").await;
    assert_eq!(
        serde_json::from_str::<Value>(&described).unwrap()["picture"],
        Value::Null
    );
}

#[tokio::test]
async fn only_its_manager_may_change_a_picture() {
    let pictured = pictured().await;
    let (put, _, _) = fetch(
        &pictured.app,
        "PUT",
        "/@grafana/picture",
        &pictured.chris,
        &[],
        png(16, 16),
    )
    .await;
    let (delete, _, _) = fetch(
        &pictured.app,
        "DELETE",
        "/@grafana/picture",
        &pictured.chris,
        &[],
        vec![],
    )
    .await;
    assert_eq!(
        (put, delete),
        (StatusCode::FORBIDDEN, StatusCode::FORBIDDEN)
    );
}

#[tokio::test]
async fn a_member_may_give_its_group_a_picture() {
    let app = TestApp::new().await;
    let chris = individual(&app, "chris").await;
    request(&app, "PUT", "/@household", &chris, &[], "").await;
    let (status, _, body) = fetch(&app, "PUT", "/@household/picture", &chris, &[], png(8, 8)).await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["kind"], json!("group"));
}

/// After `@grafana` renames itself `@dashboards`, sends `method` to its
/// old picture's URL as `@grafana`, and returns the status and where it
/// redirects.
async fn at_a_former_name(method: &str, body: Vec<u8>) -> (StatusCode, String) {
    let pictured = pictured().await;
    let rename = r#"{"name": "dashboards"}"#;
    let json = ("content-type", "application/json");
    request(
        &pictured.app,
        "PATCH",
        "/@grafana",
        &pictured.grafana,
        &[json],
        rename,
    )
    .await;
    let (status, headers, _) = fetch(
        &pictured.app,
        method,
        "/@grafana/picture",
        &pictured.grafana,
        &[],
        body,
    )
    .await;
    (status, headers["location"].to_str().unwrap().to_owned())
}

#[tokio::test]
async fn a_former_name_redirects_to_the_picture() {
    let moved = at_a_former_name("GET", vec![]).await;
    let expected = (
        StatusCode::PERMANENT_REDIRECT,
        "/@dashboards/picture".to_owned(),
    );
    assert_eq!(moved, expected);
}

#[tokio::test]
async fn putting_at_a_former_name_redirects() {
    let moved = at_a_former_name("PUT", png(16, 16)).await;
    assert_eq!(
        moved,
        (StatusCode::PERMANENT_REDIRECT, "/@dashboards".to_owned())
    );
}

#[tokio::test]
async fn removing_at_a_former_name_redirects() {
    let moved = at_a_former_name("DELETE", vec![]).await;
    assert_eq!(
        moved,
        (StatusCode::PERMANENT_REDIRECT, "/@dashboards".to_owned())
    );
}

#[tokio::test]
async fn no_one_has_a_picture_at_a_free_name() {
    let pictured = pictured().await;
    let (status, _, _) = fetch(
        &pictured.app,
        "GET",
        "/@nobody/picture",
        &pictured.chris,
        &[],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_identity_without_a_picture_has_none_to_serve() {
    let pictured = pictured().await;
    let (status, _, _) = fetch(
        &pictured.app,
        "GET",
        "/@chris/picture",
        &pictured.chris,
        &[],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
