//! The devices page: an identity's devices, and the code a phone scans to
//! find the server.

use aws_lc_rs::agreement::{ECDH_P256, PrivateKey};
use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::json;

use crate::app::testing::{TestApp, request};

const HTML: (&str, &str) = ("accept", "text/html,application/xhtml+xml");
const JSON: (&str, &str) = ("content-type", "application/json");

async fn individual(app: &TestApp, name: &str) -> String {
    let token = app.issuer.token(&ulid::Ulid::generate().to_string());
    request(app, "PUT", &format!("/@{name}"), &token, &[], "").await;
    token
}

/// A browser's device, subscribed with a real P-256 key.
fn browser() -> String {
    let key = PrivateKey::generate(&ECDH_P256).unwrap();
    let public = key.compute_public_key().unwrap();
    json!({
        "name": "A browser on Linux",
        "wake": { "web_push": {
            "endpoint": "https://push.example.com/send/abc",
            "keys": {
                "p256dh": URL_SAFE_NO_PAD.encode(public.as_ref()),
                "auth": URL_SAFE_NO_PAD.encode(b"sixteen octets!!"),
            },
        } },
    })
    .to_string()
}

/// `@chris`, with a browser and a phone among his devices.
async fn with_devices(app: &TestApp) -> String {
    let chris = individual(app, "chris").await;
    let phone = json!({ "name": "Pixel 9", "wake": { "connection": true } }).to_string();
    let tablet = json!({ "name": "Pixel Tablet", "wake": { "fcm": "a:token" } }).to_string();
    for (id, device) in [("laptop", browser()), ("phone", phone), ("tablet", tablet)] {
        let path = format!("/@chris/devices/{id}");
        let (status, _, _) = request(app, "PUT", &path, &chris, &[JSON], device).await;
        assert_eq!(status, StatusCode::CREATED);
    }
    chris
}

async fn page(app: &TestApp, token: &str) -> (StatusCode, String) {
    let (status, _, page) = request(app, "GET", "/@chris/devices", token, &[HTML], "").await;
    (status, page)
}

#[tokio::test]
async fn the_page_links_where_to_get_the_app() {
    let app = TestApp::new().await;
    let chris = with_devices(&app).await;
    let (_, page) = page(&app, &chris).await;
    for expected in [
        "https://github.com/chrisguidry/transom/releases/latest",
        "https://apps.obtainium.imranr.dev/redirect?r=obtainium://add/https://github.com/chrisguidry/transom",
    ] {
        assert!(page.contains(expected), "{expected} in {page}");
    }
}

#[tokio::test]
async fn the_page_lists_each_kind_of_device() {
    let app = TestApp::new().await;
    let chris = with_devices(&app).await;
    let (status, page) = page(&app, &chris).await;
    assert_eq!(status, StatusCode::OK);
    for expected in [
        "<h1>Devices</h1>",
        "A browser on Linux",
        "Web Push",
        "Pixel 9",
        "Direct connection",
        "Pixel Tablet",
        "Google Play services",
        r#"data-device="laptop""#,
    ] {
        assert!(page.contains(expected), "{expected} in {page}");
    }
}

#[tokio::test]
async fn a_page_without_devices_says_so() {
    let app = TestApp::new().await;
    let chris = individual(&app, "chris").await;
    let (_, page) = page(&app, &chris).await;
    assert!(page.contains("No devices yet."), "{page}");
}

#[tokio::test]
async fn only_the_identity_sees_its_devices() {
    let app = TestApp::new().await;
    with_devices(&app).await;
    let bot = individual(&app, "review-bot").await;
    let (status, page) = page(&app, &bot).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(!page.contains("Pixel 9"), "{page}");
}

#[tokio::test]
async fn forget_deletes_the_device_and_the_page_no_longer_lists_it() {
    let app = TestApp::new().await;
    let chris = with_devices(&app).await;
    let (_, before) = page(&app, &chris).await;
    assert!(
        before.contains(r#"hx-delete="/@chris/devices/phone""#),
        "{before}"
    );
    let htmx = ("hx-request", "true");
    let (status, _, _) =
        request(&app, "DELETE", "/@chris/devices/phone", &chris, &[htmx], "").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, after) = page(&app, &chris).await;
    assert!(
        !after.contains("Pixel 9") && after.contains("A browser on Linux"),
        "{after}"
    );
}

/// What the page's QR code says, read back from its SVG: each `h` in the
/// path is a run of dark modules, which this paints into a bitmap for a
/// decoder to read as a camera's picture would be read.
fn scanned(page: &str) -> String {
    let start = page.find(r#"<svg class="qr""#).unwrap();
    let svg = &page[start..start + page[start..].find("</svg>").unwrap()];
    let view_box = svg.split("viewBox=\"0 0 ").nth(1).unwrap();
    let size: usize = view_box.split(' ').next().unwrap().parse().unwrap();
    let path = svg
        .split(" d=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let mut dark = vec![false; size * size];
    for run in path.split('M').filter(|run| !run.is_empty()) {
        let (at, rest) = run.split_once('h').unwrap();
        let (x, y) = at.split_once(' ').unwrap();
        let (x, y): (usize, usize) = (x.parse().unwrap(), y.parse().unwrap());
        let length: usize = rest.split('v').next().unwrap().parse().unwrap();
        dark[y * size + x..y * size + x + length].fill(true);
    }
    let scale = 4;
    let mut image = rqrr::PreparedImage::prepare_from_bitmap(size * scale, size * scale, |x, y| {
        dark[(y / scale) * size + x / scale]
    });
    let grids = image.detect_grids();
    assert_eq!(grids.len(), 1);
    grids[0].decode().unwrap().1
}

#[tokio::test]
async fn the_pairing_code_holds_the_servers_public_url() {
    let app = TestApp::reached_at("https://transom.example.com/").await;
    let chris = with_devices(&app).await;
    let (_, page) = page(&app, &chris).await;
    assert_eq!(scanned(&page), "https://transom.example.com");
    assert!(
        page.contains(r#"<p class="pair-url">https://transom.example.com</p>"#),
        "{page}"
    );
}

#[tokio::test]
async fn without_a_public_url_the_pairing_code_holds_the_requests_origin() {
    let app = TestApp::new().await;
    let chris = individual(&app, "chris").await;
    let host = ("host", "transom.local:8080");
    let (_, _, page) = request(&app, "GET", "/@chris/devices", &chris, &[HTML, host], "").await;
    assert_eq!(scanned(&page), "http://transom.local:8080");
}

#[tokio::test]
async fn the_bar_links_to_the_devices_page() {
    let app = TestApp::new().await;
    let chris = individual(&app, "chris").await;
    let (_, _, inbox) = request(&app, "GET", "/@chris/exchanges", &chris, &[HTML], "").await;
    assert!(
        inbox.contains(r#"<a href="/@chris/devices">Devices</a>"#),
        "{inbox}"
    );
}
