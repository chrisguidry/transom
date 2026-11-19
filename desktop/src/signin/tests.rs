//! Sign-in's URLs, and signing in to a real server through a real
//! issuer, with the browser's part done over plain HTTP.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;
use crate::sync::client::Client;
use crate::testserver::{NOWHERE, TestServer};

#[test]
fn a_typed_address_becomes_the_servers_url() {
    let cases = [
        ("transom.example.com", "https://transom.example.com/"),
        (
            " https://transom.example.com/ ",
            "https://transom.example.com/",
        ),
        ("http://localhost:8080", "http://localhost:8080/"),
        (
            "transom.example.com/base?x=1#y",
            "https://transom.example.com/base",
        ),
    ];
    for (typed, url) in cases {
        assert_eq!(server_url(typed).unwrap().as_str(), url);
    }
    for typed in ["", "   ", "ftp://example.com", "https://"] {
        assert_eq!(server_url(typed), Err(Problem::NotAnAddress));
    }
}

#[test]
fn the_login_url_carries_the_challenge_and_the_callback() {
    let server = server_url("transom.example.com").unwrap();
    assert_eq!(
        login_url(&server, "abc").as_str(),
        "https://transom.example.com/login?app=abc&callback=sh.transom%3A%2Fcallback"
    );
}

#[test]
fn a_challenge_is_the_digest_of_its_verifier() {
    let pkce = Pkce::new();
    assert_eq!(pkce.challenge.len(), 43);
    assert_eq!(
        pkce.challenge,
        URL_SAFE_NO_PAD.encode(digest(&SHA256, pkce.verifier.as_bytes()))
    );
    assert_ne!(pkce.verifier, Pkce::new().verifier);
}

#[test]
fn the_code_is_read_from_the_apps_callback_only() {
    assert_eq!(
        code_in("sh.transom:/callback?code=abc").as_deref(),
        Some("abc")
    );
    assert_eq!(code_in("https://evil.example/callback?code=abc"), None);
    assert_eq!(code_in("sh.transom:/callback"), None);
}

#[test]
fn each_problem_is_told_in_words() {
    let cases = [
        (Problem::NotAnAddress, "That is not a server's address."),
        (
            Problem::Unreachable("refused".to_owned()),
            "The server could not be reached: refused",
        ),
        (
            Problem::NotTransom,
            "That address did not answer as a transom server.",
        ),
        (
            Problem::Refused,
            "The server refused the sign-in. Try again.",
        ),
        (
            Problem::Unexpected,
            "That sign-in was not started here. Try again.",
        ),
    ];
    for (problem, said) in cases {
        assert_eq!(problem.to_string(), said);
    }
}

#[test]
fn a_server_that_does_not_allow_the_callback_is_told_with_the_setting_to_change() {
    let said = Problem::CallbackNotAllowed.to_string();
    assert!(said.ends_with("has to add sh.transom:/callback to its app_callbacks setting."));
}

#[tokio::test]
async fn a_transom_server_passes_the_check() {
    let server = TestServer::start().await;
    assert_eq!(check(&server.url, &Pkce::new().challenge).await, Ok(()));
}

#[tokio::test]
async fn a_server_whose_settings_leave_the_callback_out_is_told_apart() {
    let callbacks = [("TRANSOM_APP_CALLBACKS", "https://transom.sh/app/callback")];
    let server = TestServer::with(&callbacks).await;
    let checked = check(&server.url, &Pkce::new().challenge).await;
    assert_eq!(checked, Err(Problem::CallbackNotAllowed));
}

#[tokio::test]
async fn an_address_where_nothing_listens_is_unreachable() {
    let server = Url::parse(NOWHERE).unwrap();
    let checked = check(&server, &Pkce::new().challenge).await;
    assert!(matches!(checked, Err(Problem::Unreachable(_))));
}

#[tokio::test]
async fn a_server_that_is_not_transom_fails_each_step() {
    let server = TestServer::start().await;
    let other = server.issuer();
    let checked = check(&other, &Pkce::new().challenge).await;
    assert_eq!(checked, Err(Problem::NotTransom));
    let redeemed = redeem(&other, "code", "verifier").await;
    assert_eq!(redeemed, Err(Problem::NotTransom));
    assert_eq!(name_of(&other, "token").await, Err(Problem::NotTransom));
}

/// A web server that answers every request with the same page, as the
/// captive portal of a hotel's network does, and its URL.
async fn portal() -> Url {
    const PAGE: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nSign in";
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (mut connection, _) = listener.accept().await.unwrap();
            // One read takes the whole of each small request.
            let _ = connection.read(&mut [0; 4096]).await.unwrap();
            connection.write_all(PAGE).await.unwrap();
        }
    });
    Url::parse(&url).unwrap()
}

#[tokio::test]
async fn a_page_that_is_not_transoms_fails_each_step() {
    let portal = portal().await;
    let checked = check(&portal, &Pkce::new().challenge).await;
    assert_eq!(checked, Err(Problem::NotTransom));
    let redeemed = redeem(&portal, "code", "verifier").await;
    assert_eq!(redeemed, Err(Problem::NotTransom));
    assert_eq!(name_of(&portal, "token").await, Err(Problem::NotTransom));
}

/// Signs `@bob` in to `server` as the app does, with the browser's part
/// done by the fixture, and returns the session's token.
async fn signed_in(server: &TestServer) -> String {
    let pkce = Pkce::new();
    let login = login_url(&server.url, &pkce.challenge);
    let callback = server.browse(login.as_str(), "bob").await;
    let code = code_in(&callback).unwrap();
    redeem(&server.url, &code, &pkce.verifier).await.unwrap()
}

#[tokio::test]
async fn a_sign_in_through_the_browser_gives_a_session_as_the_person() {
    let server = TestServer::start().await;
    let token = signed_in(&server).await;
    assert_eq!(name_of(&server.url, &token).await.unwrap(), "@bob");
}

#[tokio::test]
async fn a_code_with_another_verifier_is_refused() {
    let server = TestServer::start().await;
    let login = login_url(&server.url, &Pkce::new().challenge);
    let callback = server.browse(login.as_str(), "bob").await;
    let code = code_in(&callback).unwrap();
    let redeemed = redeem(&server.url, &code, &Pkce::new().verifier).await;
    assert_eq!(redeemed, Err(Problem::Refused));
}

#[tokio::test]
async fn a_token_the_server_does_not_take_names_no_one() {
    let server = TestServer::start().await;
    let named = name_of(&server.url, "not-a-token").await;
    assert_eq!(named, Err(Problem::Refused));
}

#[tokio::test]
async fn signing_out_ends_the_session_on_the_server() {
    let server = TestServer::start().await;
    let token = signed_in(&server).await;
    Client::new(&server.url, &token).sign_out().await;
    assert_eq!(name_of(&server.url, &token).await, Err(Problem::Refused));
}
