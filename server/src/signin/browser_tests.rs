//! Browser sign-in end to end: the redirect to the issuer, the callback,
//! the session cookie, refreshing it, and signing out.

use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode};

use super::test_issuer::{Refreshing, TestIssuer};
use super::{Issuer, Issuers};
use crate::app::App;
use crate::app::testing::{TestApp, request};
use crate::store::testing::store;

pub(super) async fn browser_app(secret: Option<&str>, public_url: Option<&str>) -> TestApp {
    let issuer = TestIssuer::start().await;
    let issuers = Issuers::of(vec![Issuer::new(issuer.browser_settings(secret)).unwrap()]);
    let store = store();
    crate::identities::records::register_transom(&store)
        .await
        .unwrap();
    let app = App::new(store, issuers).reached_at(public_url.map(str::to_owned));
    TestApp {
        app: Arc::new(app),
        issuer: Arc::new(issuer),
    }
}

/// The value of the `Set-Cookie` that sets `name`.
pub(super) fn set_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all("set-cookie")
        .iter()
        .map(|value| value.to_str().unwrap())
        .find(|value| value.starts_with(&format!("{name}=")))
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap()
                .split_once('=')
                .unwrap()
                .1
                .to_owned()
        })
}

pub(super) fn query(location: &str, name: &str) -> String {
    url::Url::parse(location)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .unwrap()
}

/// Begins a sign-in, and returns the state, the nonce, and the cookie that
/// binds the sign-in to this browser.
async fn begin(app: &TestApp, next: &str) -> (String, String, String, String) {
    let (status, headers, _) = request(
        app,
        "GET",
        &format!("/login/test?next={next}"),
        "",
        &[("host", "transom.example.com")],
        "",
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = headers["location"].to_str().unwrap().to_owned();
    let login = set_cookie(&headers, "__Host-transom-login").unwrap();
    (
        query(&location, "state"),
        query(&location, "nonce"),
        login,
        location,
    )
}

/// Signs in as `subject` named `name`, and returns the session cookie.
async fn sign_in(app: &TestApp, subject: &str, name: &str) -> String {
    let (state, nonce, login, _) = begin(app, "/").await;
    let path = format!("/login/callback?code={subject}%7C{name}%7C{nonce}&state={state}");
    let cookie = format!("__Host-transom-login={login}");
    let (status, headers, _) = request(app, "GET", &path, "", &[("cookie", &cookie)], "").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    set_cookie(&headers, "__Host-Http-transom").unwrap()
}

fn session(cookie: &str) -> String {
    format!("theme=dark; __Host-Http-transom={cookie}")
}

#[tokio::test]
async fn sends_the_browser_to_the_issuer_with_pkce() {
    let app = browser_app(Some("secret"), None).await;
    let (_, _, _, location) = begin(&app, "/@chris/exchanges").await;
    assert!(location.starts_with(&format!("{}/authorize?", app.issuer.url)));
    assert_eq!(query(&location, "client_id"), "transom-browser");
    assert_eq!(
        query(&location, "redirect_uri"),
        "http://transom.example.com/login/callback"
    );
    assert_eq!(query(&location, "code_challenge_method"), "S256");
    assert_eq!(query(&location, "response_type"), "code");
}

#[tokio::test]
async fn uses_the_public_url_when_there_is_one() {
    let app = browser_app(None, Some("https://transom.example.com/")).await;
    let (_, _, _, location) = begin(&app, "/").await;
    assert_eq!(
        query(&location, "redirect_uri"),
        "https://transom.example.com/login/callback"
    );
}

#[tokio::test]
async fn a_signed_in_browser_is_its_person() {
    let app = browser_app(Some("secret"), None).await;
    let cookie = sign_in(&app, "chris-subject", "chris").await;
    let (status, _, text) = request(
        &app,
        "GET",
        "/@chris",
        "",
        &[("cookie", &session(&cookie))],
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(text.contains("individual"));
}

#[tokio::test]
async fn returns_only_to_pages_of_its_own() {
    let app = browser_app(None, None).await;
    let (state, nonce, login, _) = begin(&app, "https://evil.example.com").await;
    let path = format!("/login/callback?code=s%7Cchris%7C{nonce}&state={state}");
    let (_, headers, _) = request(
        &app,
        "GET",
        &path,
        "",
        &[("cookie", &format!("__Host-transom-login={login}"))],
        "",
    )
    .await;
    assert_eq!(headers["location"], "/");
}

#[tokio::test]
async fn a_cookie_needs_htmxs_header_to_change_anything() {
    let app = browser_app(Some("secret"), None).await;
    let cookie = sign_in(&app, "chris-subject", "chris").await;
    let cookie = session(&cookie);
    request(&app, "GET", "/@chris", "", &[("cookie", &cookie)], "").await;
    let (forged, _, _) = request(&app, "PUT", "/@household", "", &[("cookie", &cookie)], "").await;
    let (htmx, _, _) = request(
        &app,
        "PUT",
        "/@household",
        "",
        &[("cookie", &cookie), ("hx-request", "true")],
        "",
    )
    .await;
    assert_eq!((forged, htmx), (StatusCode::FORBIDDEN, StatusCode::CREATED));
}

#[tokio::test]
async fn refuses_callbacks_that_do_not_finish_this_sign_in() {
    let app = browser_app(None, None).await;
    let (state, nonce, login, _) = begin(&app, "/").await;
    let bound = format!("__Host-transom-login={login}");
    let cases = [
        (
            format!("/login/callback?error=access_denied&state={state}"),
            bound.clone(),
            StatusCode::UNAUTHORIZED,
        ),
        (
            format!("/login/callback?state={state}"),
            bound.clone(),
            StatusCode::BAD_REQUEST,
        ),
        (
            format!("/login/callback?code=s%7Cchris%7C{nonce}&state={state}"),
            "__Host-transom-login=other".to_owned(),
            StatusCode::UNAUTHORIZED,
        ),
        (
            format!("/login/callback?code=s%7Cchris%7Cwrong-nonce&state={state}"),
            bound.clone(),
            StatusCode::UNAUTHORIZED,
        ),
        (
            format!("/login/callback?code=s%7Cchris%7C{nonce}&state={state}"),
            bound.clone(),
            StatusCode::UNAUTHORIZED,
        ),
    ];
    for (path, cookie, expected) in cases {
        let (status, _, _) = request(&app, "GET", &path, "", &[("cookie", &cookie)], "").await;
        assert_eq!(status, expected, "{path}");
    }
}

#[tokio::test]
async fn reports_an_issuer_that_will_not_trade_the_code() {
    let app = browser_app(None, None).await;
    for code in ["refused", "no-id-token"] {
        let (state, _, login, _) = begin(&app, "/").await;
        let path = format!("/login/callback?code={code}&state={state}");
        let (status, _, _) = request(
            &app,
            "GET",
            &path,
            "",
            &[("cookie", &format!("__Host-transom-login={login}"))],
            "",
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{code}");
    }
}

#[tokio::test]
async fn refuses_issuers_that_cannot_sign_in_a_browser() {
    let app = browser_app(None, None).await;
    let (unknown, _, _) = request(&app, "GET", "/login/nobody", "", &[], "").await;
    app.issuer.publish_no_endpoints();
    let fresh = browser_app(None, None).await;
    fresh.issuer.publish_no_endpoints();
    let (no_endpoints, _, _) = request(&fresh, "GET", "/login/test", "", &[], "").await;
    assert_eq!(
        (unknown, no_endpoints),
        (StatusCode::NOT_FOUND, StatusCode::SERVICE_UNAVAILABLE)
    );
}

/// Marks the session behind `cookie` as last refreshed long ago.
async fn age(app: &TestApp, cookie: &str) {
    use aws_lc_rs::digest::{SHA256, digest};
    use base64::Engine;
    let hashed =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest(&SHA256, cookie.as_bytes()));
    let mut connection = app.app.store.connection().await.unwrap();
    redis::cmd("HSET")
        .arg(app.app.store.key(&format!("session:{hashed}")))
        .arg("refreshed")
        .arg(0)
        .query_async::<()>(&mut connection)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_session_refreshes_with_its_issuer_and_ends_when_refused() {
    let app = browser_app(Some("secret"), None).await;
    let cookie = sign_in(&app, "chris-subject", "chris").await;
    let header = session(&cookie);
    age(&app, &cookie).await;
    let (refreshed, _, _) = request(&app, "GET", "/@chris", "", &[("cookie", &header)], "").await;
    app.issuer.refresh(Refreshing::Fails);
    age(&app, &cookie).await;
    let (carried_on, _, _) = request(&app, "GET", "/@chris", "", &[("cookie", &header)], "").await;
    app.issuer.refresh(Refreshing::Refuses);
    age(&app, &cookie).await;
    let (refused, _, _) = request(&app, "GET", "/@chris", "", &[("cookie", &header)], "").await;
    let (ended, _, _) = request(&app, "GET", "/@chris", "", &[("cookie", &header)], "").await;
    assert_eq!(
        [refreshed, carried_on, refused, ended],
        [
            StatusCode::OK,
            StatusCode::OK,
            StatusCode::UNAUTHORIZED,
            StatusCode::UNAUTHORIZED
        ]
    );
    assert_eq!(app.issuer.refreshes(), 3);
}

#[tokio::test]
async fn a_session_ends_when_its_issuer_is_no_longer_trusted() {
    let app = browser_app(None, None).await;
    let cookie = sign_in(&app, "chris-subject", "chris").await;
    age(&app, &cookie).await;
    let untrusting = App::new(
        crate::store::Store::new(
            redis::Client::open(crate::store::testing::redis_url()).unwrap(),
            app.app.store.key("").trim_end_matches(':').to_owned(),
        ),
        Issuers::of(vec![]),
    );
    let untrusting = TestApp {
        app: Arc::new(untrusting),
        issuer: app.issuer,
    };
    let (status, _, _) = request(
        &untrusting,
        "GET",
        "/@chris",
        "",
        &[("cookie", &session(&cookie))],
        "",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signing_out_ends_the_session() {
    let app = browser_app(None, None).await;
    let cookie = sign_in(&app, "chris-subject", "chris").await;
    let header = session(&cookie);
    let (status, headers, _) = request(
        &app,
        "POST",
        "/logout",
        "",
        &[("cookie", &header), ("hx-request", "true")],
        "",
    )
    .await;
    assert_eq!(
        (status, headers["hx-redirect"].to_str().unwrap()),
        (StatusCode::OK, "/login")
    );
    let (after, _, _) = request(&app, "GET", "/@chris", "", &[("cookie", &header)], "").await;
    let (plain, plain_headers, _) =
        request(&app, "POST", "/logout", "", &[("hx-request", "true")], "").await;
    assert_eq!((after, plain), (StatusCode::UNAUTHORIZED, StatusCode::OK));
    assert_eq!(
        set_cookie(&plain_headers, "__Host-Http-transom").as_deref(),
        Some("")
    );
    let (redirect, _, _) = request(
        &app,
        "POST",
        "/logout",
        "",
        &[
            ("cookie", &session("x")),
            ("hx-request", "true"),
            ("accept", "text/html"),
        ],
        "",
    )
    .await;
    assert_eq!(redirect, StatusCode::OK);
}

#[tokio::test]
async fn signing_out_without_htmx_redirects() {
    let app = browser_app(None, None).await;
    let (status, headers, _) = request(&app, "POST", "/logout", "", &[], "").await;
    assert_eq!(
        (status, headers["location"].to_str().unwrap()),
        (StatusCode::SEE_OTHER, "/login")
    );
}

#[tokio::test]
async fn the_sign_in_page_offers_the_issuer_by_its_label() {
    let app = browser_app(None, None).await;
    let (_, _, page) = request(
        &app,
        "GET",
        "/login?next=/@chris",
        "",
        &[("accept", "text/html")],
        "",
    )
    .await;
    assert!(
        page.contains(r#"href="/login/test?next=/%40chris">Continue with Test</a>"#),
        "{page}"
    );
}

#[tokio::test]
async fn signs_in_through_an_issuer_that_signs_in_at_once() {
    let app = browser_app(Some("secret"), None).await;
    let (_, _, login, location) = begin(&app, "/").await;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let issued = http.get(&location).send().await.unwrap();
    let back = url::Url::parse(issued.headers()["location"].to_str().unwrap()).unwrap();
    let path = format!("{}?{}", back.path(), back.query().unwrap());
    let cookie = format!("__Host-transom-login={login}");
    let returned = [("host", "transom.example.com"), ("cookie", &cookie)];
    let (_, headers, _) = request(&app, "GET", &path, "", &returned, "").await;
    let session = session(&set_cookie(&headers, "__Host-Http-transom").unwrap());
    let (status, _, text) = request(&app, "GET", "/@tester", "", &[("cookie", &session)], "").await;
    assert_eq!(status, StatusCode::OK, "{text}");
}
