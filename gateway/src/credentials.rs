//! Signing in to FCM as a Google service account, with the OAuth 2.0 JWT
//! bearer grant (RFC 7523): the gateway signs a JWT with the account's
//! private key and trades it at the key's `token_uri` for an access token.
//! It keeps the access token in memory and trades again a few minutes
//! before the token expires.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Mutex;

/// The one scope the gateway asks for: sending FCM messages.
const SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";

/// How long the signed JWT is good for. Google accepts at most an hour.
const ASSERTION_LIFETIME: u64 = 60 * 60;

/// How long before an access token expires the gateway trades for another,
/// so a token never expires between being read and reaching FCM.
const RENEW_BEFORE: Duration = Duration::from_mins(5);

/// How long Google's token endpoint has to answer.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The fields of a Google service account key file that signing in reads.
#[derive(Deserialize)]
struct KeyFile {
    project_id: String,
    private_key_id: String,
    private_key: String,
    client_email: String,
    token_uri: String,
}

/// A service account, ready to sign in, and the access token it holds.
pub(crate) struct Credentials {
    /// The Firebase project that messages are sent from.
    pub(crate) project: String,
    key_id: String,
    signing: EncodingKey,
    email: String,
    token_uri: String,
    held: Mutex<Option<Held>>,
}

struct Held {
    token: String,
    expires: SystemTime,
}

#[derive(Deserialize)]
struct Granted {
    access_token: String,
    expires_in: u64,
}

impl Credentials {
    /// Reads the key file at `path`: `None` when there is no file, and an
    /// error when there is one the gateway cannot use. A missing file means
    /// the gateway was deployed before its Firebase project was, so it runs
    /// and refuses wake-ups. A broken one is a mistake to stop for.
    pub(crate) fn read(path: &Path) -> Result<Option<Self>, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("{} cannot be read: {error}", path.display())),
        };
        Self::parse(&text)
            .map(Some)
            .map_err(|error| format!("{} is not a service account key: {error}", path.display()))
    }

    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let key: KeyFile = serde_json::from_str(text).map_err(|error| error.to_string())?;
        let signing = EncodingKey::from_rsa_pem(key.private_key.as_bytes())
            .map_err(|error| error.to_string())?;
        Ok(Self {
            project: key.project_id,
            key_id: key.private_key_id,
            signing,
            email: key.client_email,
            token_uri: key.token_uri,
            held: Mutex::new(None),
        })
    }

    /// An access token good until well after `now`: the one held, or a new
    /// one. Concurrent callers wait for one trade instead of each making
    /// their own.
    pub(crate) async fn access_token(
        &self,
        http: &reqwest::Client,
        now: SystemTime,
    ) -> Result<String, String> {
        let mut held = self.held.lock().await;
        if let Some(token) = held
            .as_ref()
            .filter(|held| now + RENEW_BEFORE < held.expires)
        {
            return Ok(token.token.clone());
        }
        let granted = self.trade(http, now).await?;
        let token = granted.access_token.clone();
        *held = Some(Held {
            token: granted.access_token,
            expires: now + Duration::from_secs(granted.expires_in),
        });
        Ok(token)
    }

    /// Drops the access token held, when FCM no longer takes it, so the
    /// next wake-up signs in again.
    pub(crate) async fn forget(&self, token: &str) {
        let mut held = self.held.lock().await;
        if held.as_ref().is_some_and(|held| held.token == token) {
            *held = None;
        }
    }

    async fn trade(&self, http: &reqwest::Client, now: SystemTime) -> Result<Granted, String> {
        let response = http
            .post(&self.token_uri)
            .timeout(TIMEOUT)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &self.assertion(now)),
            ])
            .send()
            .await
            .map_err(|error| format!("Google's token endpoint could not be reached: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            // Google's error body names the problem, such as a key that was
            // deleted, and holds nothing secret.
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Google's token endpoint answered {status}: {body}"));
        }
        response
            .json()
            .await
            .map_err(|error| format!("Google's token endpoint answered with no token: {error}"))
    }

    /// The JWT that asks Google for an access token, signed at `now`.
    fn assertion(&self, now: SystemTime) -> String {
        let issued = now
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after 1970")
            .as_secs();
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.key_id.clone());
        let claims = json!({
            "iss": self.email,
            "scope": SCOPE,
            "aud": self.token_uri,
            "iat": issued,
            "exp": issued + ASSERTION_LIFETIME,
        });
        jsonwebtoken::encode(&header, &claims, &self.signing).expect("an RSA key signs")
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::Credentials;
    use crate::stand_in::{PROJECT, StandIn, key_file};

    /// The test account, signing in to `stand_in`.
    fn credentials(stand_in: &StandIn) -> Credentials {
        Credentials::parse(&key_file(&stand_in.token_uri())).unwrap()
    }

    /// How many grants it takes to have an access token at `now` and again
    /// `later`, with tokens that last an hour.
    async fn grants_for(later: Duration) -> u32 {
        let stand_in = StandIn::start().await;
        let credentials = credentials(&stand_in);
        let http = reqwest::Client::new();
        let now = SystemTime::now();
        credentials.access_token(&http, now).await.unwrap();
        credentials.access_token(&http, now + later).await.unwrap();
        stand_in.grants()
    }

    #[tokio::test]
    async fn an_access_token_is_kept_while_it_has_long_to_live() {
        // A minute before the five minutes in which it is renewed.
        let later = Duration::from_mins(54);
        assert_eq!(grants_for(later).await, 1);
    }

    #[tokio::test]
    async fn an_access_token_is_renewed_before_it_expires() {
        // A minute into the five minutes in which it is renewed.
        let later = Duration::from_mins(56);
        assert_eq!(grants_for(later).await, 2);
    }

    #[tokio::test]
    async fn an_access_token_that_expires_too_soon_is_never_kept() {
        let stand_in = StandIn::granting_for(60).await;
        let credentials = credentials(&stand_in);
        let http = reqwest::Client::new();
        let now = SystemTime::now();
        credentials.access_token(&http, now).await.unwrap();
        credentials.access_token(&http, now).await.unwrap();
        assert_eq!(stand_in.grants(), 2);
    }

    #[tokio::test]
    async fn forgetting_a_token_already_replaced_keeps_the_one_held() {
        let stand_in = StandIn::start().await;
        let credentials = credentials(&stand_in);
        let http = reqwest::Client::new();
        let now = SystemTime::now();
        credentials.access_token(&http, now).await.unwrap();
        credentials.forget("access-0").await;
        credentials.access_token(&http, now).await.unwrap();
        assert_eq!(stand_in.grants(), 1);
    }

    /// The error from signing in at `token_uri`.
    async fn failure_at(token_uri: &str) -> String {
        let credentials = Credentials::parse(&key_file(token_uri)).unwrap();
        credentials
            .access_token(&reqwest::Client::new(), SystemTime::now())
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn an_unreachable_token_endpoint_is_an_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let token_uri = format!("http://{}/token", listener.local_addr().unwrap());
        drop(listener);
        let error = failure_at(&token_uri).await;
        assert!(error.contains("could not be reached"), "{error}");
    }

    #[tokio::test]
    async fn a_token_endpoint_that_grants_nothing_is_an_error() {
        let stand_in = StandIn::start().await;
        let error = failure_at(&format!("{}/nothing", stand_in.url())).await;
        assert!(error.contains("answered with no token"), "{error}");
    }

    #[test]
    fn reads_the_project_from_the_key() {
        let file =
            std::env::temp_dir().join(format!("transom-gateway-{}.json", std::process::id()));
        std::fs::write(&file, key_file("http://127.0.0.1:1/token")).unwrap();
        let credentials = Credentials::read(&file);
        std::fs::remove_file(&file).unwrap();
        assert_eq!(credentials.unwrap().unwrap().project, PROJECT);
    }

    #[test]
    fn a_missing_key_file_is_no_credentials() {
        let credentials = Credentials::read(std::path::Path::new("/nonexistent/key.json"));
        assert!(credentials.unwrap().is_none());
    }

    #[test]
    fn a_key_file_that_cannot_be_read_is_an_error() {
        let error = Credentials::read(&std::env::temp_dir()).err().unwrap();
        assert!(error.contains("cannot be read"), "{error}");
    }

    #[test]
    fn a_file_that_is_not_a_key_is_an_error() {
        let error = Credentials::parse(r#"{"project_id": "p"}"#).err().unwrap();
        assert!(error.contains("missing field"), "{error}");
    }

    #[test]
    fn a_key_that_is_not_rsa_is_an_error() {
        let key = serde_json::json!({
            "project_id": "p",
            "private_key_id": "k",
            "private_key": "not a key",
            "client_email": "e",
            "token_uri": "t",
        });
        assert!(Credentials::parse(&key.to_string()).is_err());
    }
}
