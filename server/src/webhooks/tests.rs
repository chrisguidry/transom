//! Webhooks end to end: a receiver in the test captures each POST, and the
//! test checks its RFC 9421 signature against the published key.

use std::sync::{Arc, Mutex};

use aws_lc_rs::signature::{ED25519, UnparsedPublicKey};
use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde_json::{Value, json};

use super::signing::{base, content_digest};
use crate::app::testing::{TestApp, request};
fn now() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

/// A server that records each POST it receives and answers with `status`.
struct Receiver {
    url: String,
    received: Arc<Mutex<Vec<(HeaderMap, String)>>>,
    status: Arc<Mutex<StatusCode>>,
}

impl Receiver {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/hook", listener.local_addr().unwrap());
        let received = Arc::new(Mutex::new(Vec::new()));
        let status = Arc::new(Mutex::new(StatusCode::OK));
        let (got, answer) = (received.clone(), status.clone());
        let router = Router::new().route(
            "/hook",
            post(move |headers: HeaderMap, body: String| async move {
                got.lock().unwrap().push((headers, body));
                *answer.lock().unwrap()
            }),
        );
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        Self {
            url,
            received,
            status,
        }
    }

    fn bodies(&self) -> Vec<Value> {
        let received = self.received.lock().unwrap();
        received
            .iter()
            .map(|(_, body)| serde_json::from_str(body).unwrap())
            .collect()
    }
}

const JSON: (&str, &str) = ("content-type", "application/json");

async fn individual(app: &TestApp, name: &str) -> String {
    let token = app.issuer.token(&ulid::Ulid::generate().to_string());
    request(app, "PUT", &format!("/@{name}"), &token, &[], "").await;
    token
}

async fn set_webhook(app: &TestApp, token: &str, name: &str, url: &str) -> StatusCode {
    let body = json!({ "webhook": url }).to_string();
    request(app, "PATCH", &format!("/@{name}"), token, &[JSON], body)
        .await
        .0
}

/// Checks a captured POST the way a receiver would: the digest matches the
/// body, and the signature verifies over the components it names, with the
/// key the JWKS publishes.
async fn verify(app: &TestApp, url: &str, headers: &HeaderMap, body: &str) {
    assert_eq!(
        headers["content-digest"].to_str().unwrap(),
        content_digest(body.as_bytes())
    );
    let input = headers["signature-input"].to_str().unwrap();
    let parameters = input.strip_prefix("transom=").unwrap();
    let components = [
        ("@method", "POST"),
        ("@target-uri", url),
        ("content-type", headers["content-type"].to_str().unwrap()),
        (
            "content-digest",
            headers["content-digest"].to_str().unwrap(),
        ),
    ];
    assert!(parameters.starts_with(r#"("@method" "@target-uri" "content-type" "content-digest")"#));
    let signature = headers["signature"].to_str().unwrap();
    let signature = STANDARD
        .decode(
            signature
                .strip_prefix("transom=:")
                .unwrap()
                .strip_suffix(':')
                .unwrap(),
        )
        .unwrap();
    let (_, _, jwks) = request(app, "GET", "/.well-known/jwks.json", "", &[], "").await;
    let jwks: Value = serde_json::from_str(&jwks).unwrap();
    assert!(parameters.contains(&format!(
        "keyid=\"{}\"",
        jwks["keys"][0]["kid"].as_str().unwrap()
    )));
    let public = URL_SAFE_NO_PAD
        .decode(jwks["keys"][0]["x"].as_str().unwrap())
        .unwrap();
    UnparsedPublicKey::new(&ED25519, public)
        .verify(base(&components, parameters).as_bytes(), &signature)
        .unwrap();
}

#[tokio::test]
async fn a_recipient_is_woken_with_a_signed_webhook() {
    let app = TestApp::new().await;
    let receiver = Receiver::start().await;
    let agent = individual(&app, "review-bot").await;
    assert_eq!(
        set_webhook(&app, &agent, "review-bot", &receiver.url).await,
        StatusCode::OK
    );
    let sender = individual(&app, "chris").await;
    let (_, _, sent) = request(&app, "POST", "/@review-bot", &sender, &[], "Review PR #12?").await;
    assert_eq!(
        super::deliver_due(&app.app, now()).await.unwrap(),
        1,
        "the consent request"
    );
    let consent = receiver.bodies()[0]["exchange"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (headers, body) = receiver.received.lock().unwrap()[0].clone();
    verify(&app, &receiver.url, &headers, &body).await;
    request(
        &app,
        "PUT",
        &format!("/exchanges/{consent}/reply"),
        &agent,
        &[JSON],
        r#"{"choice": "allow"}"#,
    )
    .await;
    super::deliver_due(&app.app, now()).await.unwrap();
    let bodies = receiver.bodies();
    let sent: Value = serde_json::from_str(&sent).unwrap();
    assert_eq!(bodies.last().unwrap()["event"], "received");
    assert_eq!(bodies.last().unwrap()["exchange"]["id"], sent["id"]);
}

#[tokio::test]
async fn a_sender_learns_its_exchange_ended() {
    let app = TestApp::new().await;
    let receiver = Receiver::start().await;
    let agent = individual(&app, "review-bot").await;
    set_webhook(&app, &agent, "review-bot", &receiver.url).await;
    let (_, _, sent) = request(&app, "POST", "/@review-bot", &agent, &[], "Note to self").await;
    let sent: Value = serde_json::from_str(&sent).unwrap();
    let id = sent["id"].as_str().unwrap();
    request(
        &app,
        "PUT",
        &format!("/exchanges/{id}/reply"),
        &agent,
        &[],
        "done",
    )
    .await;
    super::deliver_due(&app.app, now()).await.unwrap();
    let events: Vec<_> = receiver
        .bodies()
        .iter()
        .map(|body| body["event"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(events, ["received", "ended"]);
}

#[tokio::test]
async fn every_member_of_a_group_is_woken() {
    let app = TestApp::new().await;
    let receiver = Receiver::start().await;
    let chris = individual(&app, "chris").await;
    request(&app, "PUT", "/@household", &chris, &[], "").await;
    set_webhook(&app, &chris, "chris", &receiver.url).await;
    set_webhook(&app, &chris, "household", &receiver.url).await;
    request(&app, "POST", "/@household", &chris, &[], "Dinner at 6").await;
    super::deliver_due(&app.app, now()).await.unwrap();
    assert_eq!(receiver.bodies().len(), 2);
}

/// Queues `body` for `url`, due at `due`, as an announcement does.
async fn queue(app: &TestApp, url: &str, body: &str, due: i64) {
    let mut pipe = redis::pipe();
    pipe.atomic();
    super::enqueue(&mut pipe, &app.app, url, body, due);
    let mut connection = app.app.store.connection().await.unwrap();
    pipe.query_async::<()>(&mut connection).await.unwrap();
}

#[tokio::test]
async fn a_refused_delivery_is_retried_later_and_dropped_after_a_day() {
    let app = TestApp::new().await;
    let receiver = Receiver::start().await;
    *receiver.status.lock().unwrap() = StatusCode::INTERNAL_SERVER_ERROR;
    let start = now();
    queue(&app, &receiver.url, "{}", start).await;
    assert_eq!(super::deliver_due(&app.app, start).await.unwrap(), 0);
    assert_eq!(
        super::deliver_due(&app.app, start).await.unwrap(),
        0,
        "not due again yet"
    );
    assert_eq!(receiver.received.lock().unwrap().len(), 1);
    super::deliver_due(&app.app, start + 2000).await.unwrap();
    assert_eq!(receiver.received.lock().unwrap().len(), 2);
    let a_day_later = start + 24 * 60 * 60 * 1000;
    super::deliver_due(&app.app, a_day_later).await.unwrap();
    super::deliver_due(&app.app, a_day_later + 2 * 60 * 60 * 1000)
        .await
        .unwrap();
    assert_eq!(
        receiver.received.lock().unwrap().len(),
        3,
        "dropped after the day"
    );
}

#[tokio::test]
async fn an_unreachable_receiver_is_retried() {
    let app = TestApp::new().await;
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hook", closed.local_addr().unwrap());
    drop(closed);
    let start = now();
    queue(&app, &url, "{}", start).await;
    assert_eq!(super::deliver_due(&app.app, start).await.unwrap(), 0);
    let mut connection = app.app.store.connection().await.unwrap();
    let queued: usize = redis::cmd("ZCARD")
        .arg(app.app.store.key("deliveries"))
        .query_async(&mut connection)
        .await
        .unwrap();
    assert_eq!(queued, 1);
}

/// A delivery to a receiver, which another server on the same Redis
/// claimed at `start` and has yet to finish.
async fn claimed_elsewhere(start: i64) -> (TestApp, Receiver) {
    let app = TestApp::new().await;
    let receiver = Receiver::start().await;
    queue(&app, &receiver.url, "{}", start).await;
    let deliveries = &super::delivery::DELIVERIES;
    let id = deliveries
        .due(&app.app.store, start)
        .await
        .unwrap()
        .remove(0);
    let other = app.replica();
    assert!(deliveries.claim(&other.app, &id, start).await.unwrap());
    (app, receiver)
}

#[tokio::test]
async fn a_delivery_another_server_took_is_skipped() {
    let start = now();
    let (app, receiver) = claimed_elsewhere(start).await;
    assert_eq!(super::deliver_due(&app.app, start + 1000).await.unwrap(), 0);
    assert_eq!(receiver.bodies().len(), 0);
}

#[tokio::test]
async fn a_delivery_whose_server_stopped_mid_attempt_is_taken_over_after_its_lease() {
    let start = now();
    let (app, receiver) = claimed_elsewhere(start).await;
    let lease_ran_out = start + 60 * 1000;
    assert_eq!(
        super::deliver_due(&app.app, lease_ran_out).await.unwrap(),
        1
    );
    assert_eq!(receiver.bodies().len(), 1);
}

#[tokio::test]
async fn a_queued_id_with_no_delivery_is_dropped() {
    let app = TestApp::new().await;
    let mut connection = app.app.store.connection().await.unwrap();
    redis::cmd("ZADD")
        .arg(app.app.store.key("deliveries"))
        .arg(0)
        .arg("01GONE")
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    assert_eq!(super::deliver_due(&app.app, now()).await.unwrap(), 0);
    let queued: usize = redis::cmd("ZCARD")
        .arg(app.app.store.key("deliveries"))
        .query_async(&mut connection)
        .await
        .unwrap();
    assert_eq!(queued, 0);
}

#[tokio::test]
async fn webhooks_are_set_cleared_and_shown_only_to_who_manages_them() {
    let app = TestApp::new().await;
    let agent = individual(&app, "review-bot").await;
    let other = individual(&app, "chris").await;
    set_webhook(
        &app,
        &agent,
        "review-bot",
        "https://wicket.example.com/transom/review-bot",
    )
    .await;
    let (_, _, mine) = request(&app, "GET", "/@review-bot", &agent, &[], "").await;
    let (_, _, theirs) = request(&app, "GET", "/@review-bot", &other, &[], "").await;
    assert!(mine.contains("wicket.example.com"));
    assert!(!theirs.contains("wicket.example.com"));
    assert_eq!(
        set_webhook(&app, &agent, "review-bot", "").await,
        StatusCode::OK
    );
    let (_, _, cleared) = request(&app, "GET", "/@review-bot", &agent, &[], "").await;
    assert!(!cleared.contains("webhook"));
    let bad = [
        json!({ "webhook": "ftp://example.com" }),
        json!({ "webhook": "not a url" }),
        json!({}),
        json!({ "name": "Not Valid" }),
    ];
    for body in bad {
        let (status, _, _) = request(
            &app,
            "PATCH",
            "/@review-bot",
            &agent,
            &[JSON],
            body.to_string(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
}

#[tokio::test]
async fn every_server_on_one_redis_signs_with_one_key() {
    let app = TestApp::new().await;
    let (_, _, first) = request(&app, "GET", "/.well-known/jwks.json", "", &[], "").await;
    let other = crate::app::App::new(
        crate::store::Store::new(
            redis::Client::open(test_redis::url()).unwrap(),
            app.app.store.key("").trim_end_matches(':').to_owned(),
        ),
        crate::signin::Issuers::of(vec![]),
    );
    let signer = other.webhooks.signer(&other).await.unwrap();
    let first: Value = serde_json::from_str(&first).unwrap();
    assert_eq!(first["keys"][0]["kid"], signer.kid);
}

#[tokio::test]
async fn a_signing_key_transom_cannot_read_is_an_error() {
    let app = TestApp::new().await;
    let mut connection = app.app.store.connection().await.unwrap();
    redis::cmd("SET")
        .arg(app.app.store.key("signing-key"))
        .arg("bm90IGEga2V5")
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
    let (status, _, _) = request(&app, "GET", "/.well-known/jwks.json", "", &[], "").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn a_stopping_server_delivers_nothing() {
    let app = TestApp::new().await;
    let receiver = Receiver::start().await;
    queue(&app, &receiver.url, "{}", now()).await;
    app.app.stopping.cancel();
    assert_eq!(super::deliver_due(&app.app, now()).await.unwrap(), 0);
    assert_eq!(receiver.bodies().len(), 0);
}
