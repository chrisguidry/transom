//! Signing in to FCM as the service account Cloud Run runs the gateway as.
//! Cloud Run's metadata server names the project and grants that account's
//! access tokens, so the gateway holds no key. It keeps the access token in
//! memory and asks for another a few minutes before the token expires.

use std::time::{Duration, SystemTime};

use serde::Deserialize;
use tokio::sync::Mutex;

/// How long before an access token expires the gateway asks for another,
/// so a token never expires between being read and reaching FCM.
const RENEW_BEFORE: Duration = Duration::from_mins(5);

/// How long the metadata server has to answer. It runs beside the
/// gateway, so a slow answer means there is none, as outside Google Cloud.
const TIMEOUT: Duration = Duration::from_secs(5);

/// The project the gateway's service account belongs to, the one messages
/// are sent from.
const PROJECT_PATH: &str = "/computeMetadata/v1/project/project-id";

const TOKEN_PATH: &str = "/computeMetadata/v1/instance/service-accounts/default/token";

/// The service account the gateway runs as, and the access token it holds.
pub(crate) struct Credentials {
    /// The Firebase project that messages are sent from.
    pub(crate) project: String,
    metadata_url: String,
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
    /// Asks the metadata server at `metadata_url` for the project. An error
    /// means the gateway runs where there is no metadata server, such as
    /// outside Google Cloud, and so cannot send.
    pub(crate) async fn discover(
        http: &reqwest::Client,
        metadata_url: &str,
    ) -> Result<Self, String> {
        let metadata_url = metadata_url.trim_end_matches('/').to_owned();
        let project = metadata(http, &metadata_url, PROJECT_PATH)
            .await?
            .text()
            .await
            .map_err(|error| format!("the metadata server named no project: {error}"))?;
        Ok(Self {
            project,
            metadata_url,
            held: Mutex::new(None),
        })
    }

    /// An access token good until well after `now`: the one held, or a new
    /// one. Concurrent callers wait for one request instead of each making
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
        let granted: Granted = metadata(http, &self.metadata_url, TOKEN_PATH)
            .await?
            .json()
            .await
            .map_err(|error| format!("the metadata server answered with no token: {error}"))?;
        let token = granted.access_token.clone();
        *held = Some(Held {
            token: granted.access_token,
            expires: now + Duration::from_secs(granted.expires_in),
        });
        Ok(token)
    }

    /// Drops the access token held, when FCM no longer takes it, so the
    /// next wake-up asks for another.
    pub(crate) async fn forget(&self, token: &str) {
        let mut held = self.held.lock().await;
        if held.as_ref().is_some_and(|held| held.token == token) {
            *held = None;
        }
    }
}

/// The metadata server's successful answer at `path`.
async fn metadata(
    http: &reqwest::Client,
    metadata_url: &str,
    path: &str,
) -> Result<reqwest::Response, String> {
    let response = http
        .get(format!("{metadata_url}{path}"))
        .timeout(TIMEOUT)
        // The metadata server refuses a request without this header, so a
        // request forwarded from outside cannot read the account's tokens.
        .header("Metadata-Flavor", "Google")
        .send()
        .await
        .map_err(|error| format!("the metadata server could not be reached: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("the metadata server answered {status}: {body}"));
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::Credentials;
    use crate::stand_in::{PROJECT, PROJECT_PATH, StandIn};

    async fn credentials(stand_in: &StandIn) -> Credentials {
        Credentials::discover(&crate::app::http_client(), stand_in.url())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn reads_the_project_from_the_metadata_server() {
        let stand_in = StandIn::start().await;
        assert_eq!(credentials(&stand_in).await.project, PROJECT);
    }

    #[tokio::test]
    async fn the_metadata_server_refuses_a_request_without_its_header() {
        let stand_in = StandIn::start().await;
        let url = format!("{}{PROJECT_PATH}", stand_in.url());
        let response = crate::app::http_client().get(url).send().await.unwrap();
        assert_eq!(response.status(), 403);
    }

    /// The error from discovering the project at `metadata_url`.
    async fn discovery_failure(metadata_url: &str) -> String {
        Credentials::discover(&crate::app::http_client(), metadata_url)
            .await
            .err()
            .unwrap()
    }

    #[tokio::test]
    async fn an_unreachable_metadata_server_is_an_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let error = discovery_failure(&url).await;
        assert!(error.contains("could not be reached"), "{error}");
    }

    #[tokio::test]
    async fn a_metadata_server_without_the_project_is_an_error() {
        let stand_in = StandIn::start().await;
        let error = discovery_failure(&format!("{}/elsewhere", stand_in.url())).await;
        assert!(error.contains("answered 404"), "{error}");
    }

    /// How many grants it takes to have an access token at `now` and again
    /// `later`, with tokens that last an hour.
    async fn grants_for(later: Duration) -> u32 {
        let stand_in = StandIn::start().await;
        let credentials = credentials(&stand_in).await;
        let http = crate::app::http_client();
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
        let credentials = credentials(&stand_in).await;
        let http = crate::app::http_client();
        let now = SystemTime::now();
        credentials.access_token(&http, now).await.unwrap();
        credentials.access_token(&http, now).await.unwrap();
        assert_eq!(stand_in.grants(), 2);
    }

    #[tokio::test]
    async fn forgetting_a_token_already_replaced_keeps_the_one_held() {
        let stand_in = StandIn::start().await;
        let credentials = credentials(&stand_in).await;
        let http = crate::app::http_client();
        let now = SystemTime::now();
        credentials.access_token(&http, now).await.unwrap();
        credentials.forget("access-0").await;
        credentials.access_token(&http, now).await.unwrap();
        assert_eq!(stand_in.grants(), 1);
    }

    #[tokio::test]
    async fn a_metadata_server_that_grants_nothing_is_an_error() {
        let stand_in = StandIn::start().await;
        let credentials = Credentials {
            metadata_url: format!("{}/nothing", stand_in.url()),
            ..credentials(&stand_in).await
        };
        let error = credentials
            .access_token(&crate::app::http_client(), SystemTime::now())
            .await
            .unwrap_err();
        assert!(error.contains("answered with no token"), "{error}");
    }
}
