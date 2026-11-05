use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

use super::issuer::Rejection;
use super::test_issuer::{AUDIENCE, BROWSER_CLIENT, TestIssuer};
use super::{Issuer, Issuers, Verified};
use crate::app::App;
use crate::settings::IssuerSettings;
use crate::store::testing::store;

async fn verify(issuer: &Issuer, token: &str) -> Result<String, Rejection> {
    issuer.verify(token).await.map(|claims| claims.sub)
}

fn invalid(result: &Result<String, Rejection>) -> bool {
    matches!(result, Err(Rejection::Invalid(_)))
}

fn unavailable(result: &Result<String, Rejection>) -> bool {
    matches!(result, Err(Rejection::Unavailable(_)))
}

#[tokio::test]
async fn accepts_a_token_the_issuer_signed() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(test.settings()).unwrap();
    assert_eq!(
        verify(&issuer, &test.token("someone")).await.unwrap(),
        "someone"
    );
}

#[tokio::test]
async fn rejects_a_token_signed_by_another_key() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(test.settings()).unwrap();
    assert!(invalid(
        &verify(&issuer, &test.token_signed_by_stranger("someone")).await
    ));
}

#[tokio::test]
async fn rejects_a_token_for_another_audience() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(test.settings()).unwrap();
    let token = test.token_with(&json!({ "sub": "someone", "aud": "elsewhere" }));
    assert!(invalid(&verify(&issuer, &token).await));
}

#[tokio::test]
async fn rejects_an_expired_token() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(test.settings()).unwrap();
    let token = test.token_with(&json!({ "sub": "someone", "exp": 1 }));
    assert!(invalid(&verify(&issuer, &token).await));
}

#[tokio::test]
async fn rejects_a_token_signed_with_a_shared_secret() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(test.settings()).unwrap();
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &json!({ "sub": "someone", "iss": test.url, "aud": AUDIENCE, "exp": u64::MAX / 2 }),
        &jsonwebtoken::EncodingKey::from_secret(b"anyone can know this"),
    )
    .unwrap();
    assert!(invalid(&verify(&issuer, &token).await));
}

#[tokio::test]
async fn rejects_something_that_is_not_a_token() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(test.settings()).unwrap();
    assert!(invalid(&verify(&issuer, "not.a.token").await));
}

#[tokio::test]
async fn fetches_the_keys_again_for_a_key_it_has_not_seen() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(test.settings())
        .unwrap()
        .refreshing_after(Duration::ZERO);
    verify(&issuer, &test.token("someone")).await.unwrap();
    test.rotate("second", 2);
    assert_eq!(
        verify(&issuer, &test.token("someone")).await.unwrap(),
        "someone"
    );
    assert_eq!(test.key_fetches(), 2);
}

#[tokio::test]
async fn does_not_fetch_the_keys_again_too_soon() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(test.settings()).unwrap();
    verify(&issuer, &test.token("someone")).await.unwrap();
    test.rotate("second", 2);
    assert!(invalid(&verify(&issuer, &test.token("someone")).await));
    assert_eq!(test.key_fetches(), 1);
}

#[tokio::test]
async fn uses_the_only_key_for_a_token_that_names_none() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(test.settings()).unwrap();
    assert_eq!(
        verify(&issuer, &test.token_without_kid("someone"))
            .await
            .unwrap(),
        "someone"
    );
}

#[tokio::test]
async fn rejects_a_token_that_names_no_key_when_there_are_several() {
    let test = TestIssuer::start().await;
    test.rotate("second", 2);
    let issuer = Issuer::new(test.settings()).unwrap();
    assert!(invalid(
        &verify(&issuer, &test.token_without_kid("someone")).await
    ));
}

#[tokio::test]
async fn reports_an_issuer_it_cannot_reach() {
    let test = TestIssuer::start().await;
    test.fail();
    let issuer = Issuer::new(test.settings()).unwrap();
    assert!(unavailable(&verify(&issuer, &test.token("someone")).await));
}

#[tokio::test]
async fn refuses_a_discovery_document_for_another_issuer() {
    let test = TestIssuer::start().await;
    test.claim_issuer("https://impostor.example.com");
    let issuer = Issuer::new(test.settings()).unwrap();
    assert!(unavailable(&verify(&issuer, &test.token("someone")).await));
}

fn temporary_file(contents: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("transom-{}", ulid::Ulid::generate()));
    std::fs::write(&path, contents).unwrap();
    path
}

#[tokio::test]
async fn sends_the_token_file_when_fetching_keys() {
    let test = TestIssuer::start().await;
    let token_file = temporary_file("the-service-account-token\n");
    let issuer = Issuer::new(IssuerSettings {
        token_file: Some(token_file.clone()),
        ..test.settings()
    })
    .unwrap();
    verify(&issuer, &test.token("someone")).await.unwrap();
    std::fs::remove_file(token_file).unwrap();
    assert_eq!(
        test.authorization().as_deref(),
        Some("Bearer the-service-account-token")
    );
}

#[tokio::test]
async fn reports_a_token_file_it_cannot_read() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(IssuerSettings {
        token_file: Some("/nonexistent/token".into()),
        ..test.settings()
    })
    .unwrap();
    assert!(unavailable(&verify(&issuer, &test.token("someone")).await));
}

#[tokio::test]
async fn trusts_a_ca_file() {
    let test = TestIssuer::start().await;
    let certificate = rcgen::generate_simple_self_signed(["issuer.example.com".to_owned()])
        .unwrap()
        .cert
        .pem();
    let ca_file = temporary_file(&certificate);
    let issuer = Issuer::new(IssuerSettings {
        ca_file: Some(ca_file.clone()),
        ..test.settings()
    });
    std::fs::remove_file(ca_file).unwrap();
    assert!(issuer.is_ok());
}

#[tokio::test]
async fn fails_on_a_ca_file_it_cannot_read() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(IssuerSettings {
        ca_file: Some("/nonexistent/ca.crt".into()),
        ..test.settings()
    });
    assert!(issuer.is_err());
}

#[tokio::test]
async fn refuses_two_issuers_with_one_url() {
    let test = TestIssuer::start().await;
    assert!(Issuers::new(vec![test.settings(), test.settings()]).is_err());
    assert!(Issuers::new(vec![test.settings()]).is_ok());
}

/// A route that answers with what sign-in made of the request's token.
async fn whoami(verified: Verified) -> String {
    format!("{} {:?}", verified.subject, verified.name)
}

async fn signed_in(
    issuers: Vec<Issuer>,
    token: Option<&str>,
) -> (StatusCode, Option<String>, String) {
    let app = Arc::new(App::new(store(), Issuers::of(issuers)));
    let router = Router::new().route("/whoami", get(whoami)).with_state(app);
    let mut request = Request::get("/whoami");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = router
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let challenge = response
        .headers()
        .get("www-authenticate")
        .map(|value| value.to_str().unwrap().to_owned());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, challenge, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn names_the_caller_from_the_name_claim() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(test.settings()).unwrap();
    let token = test.token_named("subject-1", "chris");
    let (status, _, body) = signed_in(vec![issuer], Some(&token)).await;
    assert_eq!(
        (status, body.as_str()),
        (StatusCode::OK, "subject-1 Some(\"chris\")")
    );
}

#[tokio::test]
async fn has_no_name_for_an_issuer_without_a_name_claim() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(IssuerSettings {
        name_claim: None,
        ..test.settings()
    })
    .unwrap();
    let token = test.token_named("subject-1", "chris");
    let (_, _, body) = signed_in(vec![issuer], Some(&token)).await;
    assert_eq!(body, "subject-1 None");
}

#[tokio::test]
async fn challenges_a_request_without_a_token() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(test.settings()).unwrap();
    let (status, challenge, _) = signed_in(vec![issuer], None).await;
    assert_eq!(
        (status, challenge.as_deref()),
        (
            StatusCode::UNAUTHORIZED,
            Some(
                "Bearer realm=\"transom\", resource_metadata=\"http://localhost/.well-known/oauth-protected-resource\""
            )
        )
    );
}

#[tokio::test]
async fn challenges_a_request_with_something_that_is_not_a_token() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(test.settings()).unwrap();
    let (status, challenge, _) = signed_in(vec![issuer], Some("garbage")).await;
    assert_eq!(
        (status, challenge.as_deref()),
        (
            StatusCode::UNAUTHORIZED,
            Some(
                "Bearer realm=\"transom\", error=\"invalid_token\", resource_metadata=\"http://localhost/.well-known/oauth-protected-resource\""
            )
        )
    );
}

#[tokio::test]
async fn challenges_a_token_from_an_untrusted_issuer() {
    let trusted = TestIssuer::start().await;
    let untrusted = TestIssuer::start().await;
    let issuer = Issuer::new(trusted.settings()).unwrap();
    let (status, _, _) = signed_in(vec![issuer], Some(&untrusted.token("someone"))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn challenges_a_token_the_issuer_did_not_sign() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(test.settings()).unwrap();
    let token = test.token_signed_by_stranger("someone");
    let (status, _, _) = signed_in(vec![issuer], Some(&token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn is_unavailable_when_the_issuer_is() {
    let test = TestIssuer::start().await;
    test.fail();
    let issuer = Issuer::new(test.settings()).unwrap();
    let (status, _, _) = signed_in(vec![issuer], Some(&test.token("someone"))).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

/// An MCP client such as a claude.ai connector signs a person in as
/// transom's own client, so the issuer issues its access token to that
/// client. An audience equal to the client's ID accepts the token.
#[tokio::test]
async fn accepts_an_access_token_issued_to_transoms_own_client() {
    let test = TestIssuer::start().await;
    let issuer = Issuer::new(IssuerSettings {
        audience: BROWSER_CLIENT.to_owned(),
        ..test.browser_settings(None)
    })
    .unwrap();
    let token = test.token_with(&json!({ "sub": "someone", "aud": BROWSER_CLIENT }));
    let (status, _, _) = signed_in(vec![issuer], Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
}
