//! Signing in through the authorization endpoint and redeeming its code,
//! as transom's browser sign-in does, and the access tokens a script asks
//! for.

use aws_lc_rs::digest::{SHA256, digest};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use reqwest::StatusCode;
use serde_json::Value;
use test_issuer::{Client, Options, TestIssuer};

const CALLBACK: &str = "https://transom.example.com/login/callback";
const VERIFIER: &str = "a-verifier-long-enough-for-pkce-to-take-it-seriously";

/// An issuer serving on a free port, as the binary runs it.
async fn serving(options: Options) -> TestIssuer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let issuer = TestIssuer::new(url, options);
    let router = issuer.router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    issuer
}

fn http() -> reqwest::Client {
    // reqwest is built as transom builds it, without a crypto provider of
    // its own, so a client needs one installed whether or not it uses TLS.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

fn challenge() -> String {
    URL_SAFE_NO_PAD.encode(digest(&SHA256, VERIFIER.as_bytes()))
}

/// The authorization endpoint's answer to the request transom's browser
/// sign-in makes, with `extra` query parameters in place of its own.
async fn authorize(issuer: &TestIssuer, extra: &[(&str, &str)]) -> reqwest::Response {
    let challenge = challenge();
    let mut query = vec![
        ("response_type", "code"),
        ("client_id", "transom-browser"),
        ("redirect_uri", CALLBACK),
        ("scope", "openid profile email offline_access"),
        ("state", "the-state"),
        ("nonce", "the-nonce"),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
    ];
    // A repeated parameter fails to parse, so each of `extra` replaces the
    // one transom would send rather than following it.
    query.retain(|(key, _)| extra.iter().all(|(name, _)| name != key));
    query.extend_from_slice(extra);
    let url = url::Url::parse_with_params(&format!("{}/authorize", issuer.url), &query).unwrap();
    http().get(url).send().await.unwrap()
}

fn returned(location: &str, name: &str) -> String {
    url::Url::parse(location)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .unwrap()
}

/// The code an authorization with `extra` query parameters issued.
async fn code(issuer: &TestIssuer, extra: &[(&str, &str)]) -> String {
    let response = authorize(issuer, extra).await;
    returned(response.headers()["location"].to_str().unwrap(), "code")
}

/// Redeems `code` with the form transom sends, with `changes` over it,
/// as the client `transom-browser` authenticating with `secret`.
async fn redeem(
    issuer: &TestIssuer,
    code: &str,
    changes: &[(&str, &str)],
    secret: Option<&str>,
) -> reqwest::Response {
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", CALLBACK),
        ("code_verifier", VERIFIER),
    ];
    let mut request = http().post(format!("{}/token", issuer.url));
    match secret {
        Some(secret) => request = request.basic_auth("transom-browser", Some(secret)),
        None => form.push(("client_id", "transom-browser")),
    }
    // Later fields win, so a change replaces what transom would send.
    form.extend_from_slice(changes);
    request.form(&form).send().await.unwrap()
}

/// The claims of `token`, once its signature checks out against the
/// issuer's keys and it is for `audience`.
async fn verified(issuer: &TestIssuer, token: &str, audience: &str) -> Value {
    let url = format!("{}/keys", issuer.url);
    let keys: JwkSet = http().get(url).send().await.unwrap().json().await.unwrap();
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.set_audience(&[audience]);
    validation.set_issuer(&[&issuer.url]);
    let key = DecodingKey::from_jwk(&keys.keys[0]).unwrap();
    jsonwebtoken::decode(token, &key, &validation)
        .unwrap()
        .claims
}

async fn id_token(issuer: &TestIssuer, response: reqwest::Response) -> Value {
    assert_eq!(response.status(), StatusCode::OK);
    let tokens: Value = response.json().await.unwrap();
    let id_token = tokens["id_token"].as_str().unwrap();
    verified(issuer, id_token, "transom-browser").await
}

#[tokio::test]
async fn sends_the_browser_back_with_a_code_and_the_state() {
    let issuer = serving(Options::default()).await;
    let response = authorize(&issuer, &[]).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response.headers()["location"].to_str().unwrap();
    assert!(location.starts_with(&format!("{CALLBACK}?code=")));
    assert_eq!(returned(location, "state"), "the-state");
}

#[tokio::test]
async fn signs_in_the_configured_person() {
    let issuer = serving(Options::default()).await;
    let code = code(&issuer, &[]).await;
    let claims = id_token(&issuer, redeem(&issuer, &code, &[], None).await).await;
    assert_eq!(claims["sub"], "tester");
    assert_eq!(claims["preferred_username"], "tester");
    assert_eq!(claims["nonce"], "the-nonce");
}

#[tokio::test]
async fn signs_in_whoever_the_request_names() {
    let issuer = serving(Options::default()).await;
    let code = code(&issuer, &[("subject", "chris-subject"), ("name", "chris")]).await;
    let claims = id_token(&issuer, redeem(&issuer, &code, &[], None).await).await;
    assert_eq!(claims["sub"], "chris-subject");
    assert_eq!(claims["preferred_username"], "chris");
}

#[tokio::test]
async fn refuses_an_unknown_client() {
    let issuer = serving(Options::default()).await;
    let response = authorize(&issuer, &[("client_id", "someone-else")]).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn refuses_an_authorization_without_s256() {
    let issuer = serving(Options::default()).await;
    let response = authorize(&issuer, &[("code_challenge_method", "plain")]).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn refuses_a_redirect_uri_that_is_not_a_url() {
    let issuer = serving(Options::default()).await;
    let response = authorize(&issuer, &[("redirect_uri", "not a url")]).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn refuses_the_wrong_verifier() {
    let issuer = serving(Options::default()).await;
    let code = code(&issuer, &[]).await;
    let response = redeem(&issuer, &code, &[("code_verifier", "a-guess")], None).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn refuses_another_redirect_uri() {
    let issuer = serving(Options::default()).await;
    let code = code(&issuer, &[]).await;
    let changes = [("redirect_uri", "https://elsewhere.example.com/")];
    let response = redeem(&issuer, &code, &changes, None).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn refuses_a_code_redeemed_twice() {
    let issuer = serving(Options::default()).await;
    let code = code(&issuer, &[]).await;
    redeem(&issuer, &code, &[], None).await;
    let response = redeem(&issuer, &code, &[], None).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn refuses_a_code_it_did_not_issue() {
    let issuer = serving(Options::default()).await;
    let response = redeem(&issuer, "s|chris|nonce", &[], None).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

fn with_secret() -> Options {
    Options {
        client: Client {
            id: "transom-browser".to_owned(),
            secret: Some("secret".to_owned()),
        },
        ..Options::default()
    }
}

#[tokio::test]
async fn a_client_with_a_secret_authenticates_with_it() {
    let issuer = serving(with_secret()).await;
    let code = code(&issuer, &[]).await;
    let claims = id_token(&issuer, redeem(&issuer, &code, &[], Some("secret")).await).await;
    assert_eq!(claims["sub"], "tester");
}

#[tokio::test]
async fn refuses_the_wrong_secret() {
    let issuer = serving(with_secret()).await;
    let code = code(&issuer, &[]).await;
    let response = redeem(&issuer, &code, &[], Some("a-guess")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refuses_a_client_without_its_secret() {
    let issuer = serving(with_secret()).await;
    let code = code(&issuer, &[]).await;
    let response = redeem(&issuer, &code, &[], None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refuses_another_client_redeeming_the_code() {
    let issuer = serving(Options::default()).await;
    let code = code(&issuer, &[]).await;
    let response = redeem(&issuer, &code, &[("client_id", "someone-else")], None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

async fn access_token(issuer: &TestIssuer, query: &str) -> Value {
    let url = format!("{}/access-token{query}", issuer.url);
    let token = http().get(url).send().await.unwrap().text().await.unwrap();
    verified(issuer, &token, "transom-tests").await
}

#[tokio::test]
async fn gives_a_script_an_access_token_for_the_configured_person() {
    let issuer = serving(Options::default()).await;
    let claims = access_token(&issuer, "").await;
    assert_eq!(claims["sub"], "tester");
    assert_eq!(claims["preferred_username"], "tester");
}

#[tokio::test]
async fn gives_a_script_an_access_token_for_whoever_it_names() {
    let issuer = serving(Options::default()).await;
    let claims = access_token(&issuer, "?subject=chris-subject&name=chris").await;
    assert_eq!(claims["sub"], "chris-subject");
    assert_eq!(claims["preferred_username"], "chris");
}
