//! A session's devices when the session ends: each hears once that it was
//! signed out and is forgotten, a device whose session expired unused is
//! forgotten before anything is sent to it, and a browser that turns its
//! notifications off stays signed in.

use axum::http::StatusCode;
use redis::AsyncCommands;
use serde_json::{Value, json};

use super::devices::{PHONE, phone};
use super::gateway::Gateway;
use super::{Browser, JSON, Pair, PushService, devices, individual};
use crate::app::testing::{TestApp, decide, request};
use crate::exchanges::records::now;
use crate::push::{deliver_due, wake_due};
use crate::signin::SESSION_PREFIX;
use crate::signin::sessions::{self, Kind, SignedIn};

/// A session of `@chris`'s, held as `kind` holds it.
struct Session {
    token: String,
    kind: Kind,
}

impl Session {
    /// The bearer token and the headers a request made with the session
    /// carries.
    fn credentials(&self) -> (String, Vec<(&'static str, String)>) {
        match self.kind {
            Kind::App => (format!("{SESSION_PREFIX}{}", self.token), vec![]),
            Kind::Browser => (
                String::new(),
                vec![
                    ("cookie", format!("__Host-Http-transom={}", self.token)),
                    ("hx-request", "true".to_owned()),
                ],
            ),
        }
    }

    /// Sends a request with the session, with `headers`, and returns its
    /// status.
    async fn send(
        &self,
        app: &TestApp,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: String,
    ) -> StatusCode {
        let (token, carried) = self.credentials();
        let mut all: Vec<(&str, &str)> = carried
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        all.extend_from_slice(headers);
        request(app, method, path, &token, &all, body).await.0
    }

    /// Registers `device` as `@chris`'s device `id`.
    async fn register(&self, app: &TestApp, id: &str, device: String) -> StatusCode {
        let path = format!("/@chris/devices/{id}");
        self.send(app, "PUT", &path, &[JSON], device).await
    }

    async fn sign_out(&self, app: &TestApp) {
        self.send(app, "POST", "/logout", &[], String::new()).await;
    }

    /// Deletes the session, as Redis does when it expires unused.
    async fn expire(&self, app: &TestApp) {
        let key = app
            .app
            .store
            .key(&format!("session:{}", sessions::hashed(&self.token)));
        let mut connection = app.app.store.connection().await.unwrap();
        let () = connection.del(key).await.unwrap();
    }
}

/// `@review-bot`, and `@chris`, who allowed it, on `app`.
async fn chris(app: TestApp) -> (Pair, String) {
    let subject = ulid::Ulid::generate().to_string();
    let chris = app.issuer.token(&subject);
    request(&app, "PUT", "/@chris", &chris, &[], "").await;
    let bot = individual(&app, "review-bot").await;
    request(&app, "POST", "/@chris", &bot, &[], "hello").await;
    decide(&app, &chris, "chris", "allow").await;
    (Pair { app, bot, chris }, subject)
}

/// A new session of `@chris`'s, of `kind`.
async fn session(pair: &Pair, subject: &str, kind: Kind) -> Session {
    let person = SignedIn {
        issuer: pair.app.issuer.url.clone(),
        subject: subject.to_owned(),
        name: "chris".to_owned(),
        refresh: String::new(),
    };
    let token = sessions::start(&pair.app.app, &person, kind).await.unwrap();
    Session { token, kind }
}

/// Sends `@chris` a question from `@review-bot`.
async fn ask(pair: &Pair) {
    let body = json!({ "summary": "Deploy?", "choices": ["Ship it", "Wait"] }).to_string();
    request(&pair.app, "POST", "/@chris", &pair.bot, &[JSON], body).await;
}

/// What each push the browser received said.
fn said(service: &PushService, browser: &Browser) -> Vec<Value> {
    service
        .received()
        .iter()
        .map(|(_, body)| browser.read(body)["event"].clone())
        .collect()
}

/// `@chris` with a browser signed in that subscribed at a push service,
/// and `@chris`'s subject.
async fn browser_signed_in() -> (Pair, Session, PushService, Browser, String) {
    let (pair, subject) = chris(TestApp::new().await).await;
    let session = session(&pair, &subject, Kind::Browser).await;
    let service = PushService::start().await;
    let browser = Browser::at(&service.url);
    session
        .register(&pair.app, "laptop", browser.device())
        .await;
    (pair, session, service, browser, subject)
}

#[tokio::test]
async fn a_browser_hears_once_that_it_was_signed_out_and_is_forgotten() {
    let (pair, session, service, browser, _) = browser_signed_in().await;
    ask(&pair).await;
    session.sign_out(&pair.app).await;
    deliver_due(&pair.app.app, now()).await.unwrap();
    assert_eq!(said(&service, &browser), [json!("signed-out")]);
    assert_eq!(devices(&pair).await["devices"], json!([]));
}

#[tokio::test]
async fn a_device_whose_session_expired_is_forgotten_instead_of_pushed() {
    let (pair, session, service, browser, _) = browser_signed_in().await;
    session.expire(&pair.app).await;
    ask(&pair).await;
    deliver_due(&pair.app.app, now()).await.unwrap();
    assert_eq!(said(&service, &browser), Vec::<Value>::new());
    assert_eq!(devices(&pair).await["devices"], json!([]));
}

#[tokio::test]
async fn turning_off_a_browsers_notifications_leaves_it_signed_in() {
    let (pair, session, _, _, _) = browser_signed_in().await;
    let removed = session
        .send(
            &pair.app,
            "DELETE",
            "/@chris/devices/laptop",
            &[],
            String::new(),
        )
        .await;
    let after = session
        .send(&pair.app, "GET", "/@chris", &[], String::new())
        .await;
    assert_eq!((removed, after), (StatusCode::NO_CONTENT, StatusCode::OK));
}

#[tokio::test]
async fn a_device_registered_again_by_another_session_stays_when_the_first_ends() {
    let (pair, first, _, browser, subject) = browser_signed_in().await;
    let second = session(&pair, &subject, Kind::Browser).await;
    second.register(&pair.app, "laptop", browser.device()).await;
    first.sign_out(&pair.app).await;
    let listed = devices(&pair).await["devices"][0]["id"].clone();
    assert_eq!(listed, json!("laptop"));
}

#[tokio::test]
async fn a_session_that_ends_after_its_device_went_to_another_sends_it_nothing_more() {
    let (pair, first, service, browser, subject) = browser_signed_in().await;
    let second = session(&pair, &subject, Kind::Browser).await;
    second.register(&pair.app, "laptop", browser.device()).await;
    second.sign_out(&pair.app).await;
    first.sign_out(&pair.app).await;
    deliver_due(&pair.app.app, now()).await.unwrap();
    assert_eq!(said(&service, &browser), [json!("signed-out")]);
}

/// `@chris` with the app signed in on a phone that wakes by `fcm`, on a
/// transom that wakes phones through a gateway of its own.
async fn phone_signed_in(fcm: &str) -> (Pair, Session, Gateway) {
    let gateway = Gateway::start().await;
    let (pair, subject) = chris(TestApp::waking_through(&gateway.url).await).await;
    let session = session(&pair, &subject, Kind::App).await;
    session.register(&pair.app, "phone", phone(fcm)).await;
    (pair, session, gateway)
}

#[tokio::test]
async fn a_phone_is_woken_once_more_when_its_session_signs_out_and_is_forgotten() {
    let (pair, session, gateway) = phone_signed_in("a-phone").await;
    ask(&pair).await;
    session.sign_out(&pair.app).await;
    wake_due(&pair.app.app, now()).await.unwrap();
    assert_eq!(gateway.woken(), ["a-phone"]);
    assert_eq!(devices(&pair).await["devices"], json!([]));
}

#[tokio::test]
async fn a_phone_whose_session_expired_is_forgotten_instead_of_woken() {
    let (pair, session, gateway) = phone_signed_in("a-phone").await;
    session.expire(&pair.app).await;
    ask(&pair).await;
    wake_due(&pair.app.app, now()).await.unwrap();
    assert_eq!(gateway.woken(), Vec::<String>::new());
    assert_eq!(devices(&pair).await["devices"], json!([]));
}

#[tokio::test]
async fn a_phone_that_holds_a_connection_is_forgotten_when_its_session_signs_out() {
    let (pair, subject) = chris(TestApp::new().await).await;
    let session = session(&pair, &subject, Kind::App).await;
    session.register(&pair.app, "phone", PHONE.to_owned()).await;
    session.sign_out(&pair.app).await;
    assert_eq!(devices(&pair).await["devices"], json!([]));
}

#[tokio::test]
async fn removing_the_apps_phone_from_elsewhere_wakes_it_once_more_and_signs_it_out() {
    let (pair, session, gateway) = phone_signed_in("a-phone").await;
    let path = "/@chris/devices/phone";
    request(&pair.app, "DELETE", path, &pair.chris, &[], "").await;
    wake_due(&pair.app.app, now()).await.unwrap();
    let after = session
        .send(&pair.app, "GET", "/@chris", &[], String::new())
        .await;
    assert_eq!(gateway.woken(), ["a-phone"]);
    assert_eq!(after, StatusCode::UNAUTHORIZED);
    assert_eq!(devices(&pair).await["devices"], json!([]));
}
