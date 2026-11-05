//! Sending one wake-up through FCM's HTTP v1 API, and what FCM's answer
//! means for the server that asked.

use std::time::Duration;

use serde_json::{Value, json};

/// How long FCM holds a wake-up for a phone it cannot reach now. A phone
/// that comes back within this time syncs; one gone longer syncs when its
/// app next starts. The collapse key keeps FCM from holding more than one.
const TTL: &str = "900s";

/// How long FCM has to answer.
const TIMEOUT: Duration = Duration::from_secs(10);

/// What became of one wake-up.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Sent {
    /// FCM took it.
    Taken,
    /// FCM will never deliver to the token: the app was uninstalled, or
    /// the token was never one.
    Gone,
    /// FCM is busy or failing, and may take it later: after this many
    /// seconds, when FCM said.
    Busy(Option<String>),
    /// FCM no longer takes the access token the gateway sent.
    Unauthorized,
    /// FCM refused it for a reason the server cannot fix, such as a token
    /// from another Firebase project.
    Refused(String),
}

/// The message that wakes the app: data only, so Android hands it to the
/// app instead of showing anything, with nothing in it but the fact of the
/// wake-up. The app's token is its Firebase installation ID, which FCM
/// takes as `fid`, in place of the registration tokens it deprecated.
fn message(token: &str) -> Value {
    json!({
        "message": {
            "fid": token,
            "data": { "wake": "1" },
            "android": {
                "priority": "HIGH",
                "ttl": TTL,
                "collapse_key": "wake",
            },
        },
    })
}

/// Sends a wake-up to `token`, from `project`, with `access_token`, to the
/// FCM API at `fcm_url`.
pub(crate) async fn send(
    http: &reqwest::Client,
    fcm_url: &str,
    project: &str,
    access_token: &str,
    token: &str,
) -> Sent {
    let url = format!("{fcm_url}/v1/projects/{project}/messages:send");
    let sent = http
        .post(url)
        .timeout(TIMEOUT)
        .bearer_auth(access_token)
        .json(&message(token))
        .send()
        .await;
    let response = match sent {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(%error, "FCM could not be reached");
            return Sent::Busy(None);
        }
    };
    let status = response.status();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body: Value = response.json().await.unwrap_or_default();
    let error = body["error"]["status"].as_str().unwrap_or_default();
    match (status.as_u16(), error) {
        (200..=299, _) => Sent::Taken,
        // The message is the same for every token, so an argument FCM
        // finds invalid is the token.
        (404, _) | (_, "UNREGISTERED" | "INVALID_ARGUMENT") => Sent::Gone,
        (401, _) => Sent::Unauthorized,
        (429 | 500..=599, _) => Sent::Busy(retry_after),
        _ => Sent::Refused(format!("FCM answered {status} {error}")),
    }
}
