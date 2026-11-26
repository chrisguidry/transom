//! Signing in through the server, as the Android app does. The app opens
//! `/login?app={challenge}&callback={url}` in the person's browser, with a
//! PKCE S256 challenge it made, and they sign in there on their issuer's
//! own page. transom then sends the browser to the app's callback with a
//! one-time code, and the app trades the code and its PKCE verifier at
//! `/login/app` for the session's token.

pub mod keyring;
pub mod screen;

use aws_lc_rs::digest::{SHA256, digest};
use aws_lc_rs::rand;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;

use crate::sync::client::http;

/// Where the server sends the browser with the one-time code. The app's
/// desktop entry claims the `sh.transom` scheme, so the desktop starts
/// the app with this URL, and the running instance receives it.
pub const CALLBACK: &str = "sh.transom:/callback";

/// A signed-in session: the server, its token, and the person's handle
/// with its `@`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Session {
    pub server: Url,
    pub token: String,
    pub me: String,
}

/// What went wrong with a sign-in, as the sign-in screen tells the
/// person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// What they typed is no URL.
    NotAnAddress,
    /// The server did not answer, or the computer is offline.
    Unreachable(String),
    /// Something answered that is not a transom server.
    NotTransom,
    /// A transom server answered, and its settings do not list the
    /// desktop app's callback, as a server from before the desktop app
    /// does not.
    CallbackNotAllowed,
    /// The server refused the one-time code, or the session it gave.
    Refused,
    /// A callback arrived that no sign-in here was waiting for.
    Unexpected,
    /// The server ended the session, for the reason its `code` names.
    Ended(Option<String>),
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnAddress => write!(f, "That is not a server's address."),
            Self::Unreachable(cause) => write!(f, "The server could not be reached: {cause}"),
            Self::NotTransom => write!(f, "That address did not answer as a transom server."),
            Self::CallbackNotAllowed => write!(
                f,
                "This transom server does not let the desktop app sign in yet. \
                 Whoever runs it has to add {CALLBACK} to its app_callbacks setting."
            ),
            Self::Refused => write!(f, "The server refused the sign-in. Try again."),
            Self::Unexpected => write!(f, "That sign-in was not started here. Try again."),
            Self::Ended(code) => match code.as_deref() {
                Some("issuer_ended") => write!(
                    f,
                    "Your sign-in provider ended your session. Sign in again."
                ),
                Some("untrusted_issuer") => write!(
                    f,
                    "This server no longer trusts your sign-in provider. Sign in again."
                ),
                _ => write!(f, "Your session ended. Sign in again."),
            },
        }
    }
}

impl From<reqwest::Error> for Problem {
    fn from(error: reqwest::Error) -> Self {
        if error.is_decode() {
            Self::NotTransom
        } else {
            Self::Unreachable(error.without_url().to_string())
        }
    }
}

/// A PKCE pair: the verifier the app keeps, and the S256 challenge it
/// shows the server.
#[derive(Clone, Debug)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn new() -> Self {
        let mut bytes = [0u8; 32];
        rand::fill(&mut bytes).expect("the system gives random bytes");
        let verifier = URL_SAFE_NO_PAD.encode(bytes);
        let challenge = URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()));
        Self {
            verifier,
            challenge,
        }
    }
}

/// The server's URL from what a person typed: `https` unless they wrote
/// a scheme, and nothing after the host but a path.
pub fn server_url(typed: &str) -> Result<Url, Problem> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err(Problem::NotAnAddress);
    }
    let with_scheme = if typed.contains("://") {
        typed.to_owned()
    } else {
        format!("https://{typed}")
    };
    let mut url = Url::parse(&with_scheme).map_err(|_| Problem::NotAnAddress)?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return Err(Problem::NotAnAddress);
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

/// The page the browser opens to begin signing in.
pub fn login_url(server: &Url, challenge: &str) -> Url {
    let mut url = join(server, "login");
    url.query_pairs_mut()
        .append_pair("app", challenge)
        .append_pair("callback", CALLBACK);
    url
}

fn join(server: &Url, segment: &str) -> Url {
    let mut url = server.clone();
    url.path_segments_mut()
        .expect("an http URL has path segments")
        .pop_if_empty()
        .push(segment);
    url
}

/// Asks `server` for the sign-in page the browser is about to open, so a
/// mistyped address is told before a browser opens. transom's page
/// carries the app's challenge on its link to each issuer, and nothing
/// but transom would.
pub async fn check(server: &Url, challenge: &str) -> Result<(), Problem> {
    let response = http()
        .get(login_url(server, challenge))
        .header("Accept", "text/html")
        .send()
        .await?;
    let refused = response.status() == reqwest::StatusCode::BAD_REQUEST;
    let succeeded = response.status().is_success();
    let page = response.text().await?;
    // transom refuses a callback its settings do not list, and its
    // reason names the setting.
    if refused && page.contains("app_callbacks") {
        return Err(Problem::CallbackNotAllowed);
    }
    if !succeeded || !page.contains(challenge) {
        return Err(Problem::NotTransom);
    }
    Ok(())
}

/// The one-time code in the callback the desktop started the app with.
pub fn code_in(callback: &str) -> Option<String> {
    let url = Url::parse(callback).ok()?;
    if url.scheme() != "sh.transom" {
        return None;
    }
    url.query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, code)| code.into_owned())
}

/// Trades the one-time `code` and the `verifier` behind its challenge
/// for a session's token.
pub async fn redeem(server: &Url, code: &str, verifier: &str) -> Result<String, Problem> {
    #[derive(Deserialize)]
    struct Redeemed {
        token: String,
    }
    let response = http()
        .post(join(&join(server, "login"), "app"))
        .json(&json!({ "code": code, "verifier": verifier }))
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::BAD_REQUEST {
        return Err(Problem::Refused);
    }
    if !response.status().is_success() {
        return Err(Problem::NotTransom);
    }
    Ok(response.json::<Redeemed>().await?.token)
}

/// The handle that `token` signs in as, with its `@`. transom answers
/// `GET /` with a `303` to `/@{name}/exchanges`.
pub async fn name_of(server: &Url, token: &str) -> Result<String, Problem> {
    let response = http()
        .get(join(server, ""))
        .bearer_auth(token)
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(Problem::Refused);
    }
    let location = response
        .headers()
        .get("Location")
        .and_then(|location| location.to_str().ok())
        .and_then(|location| server.join(location).ok());
    let segments: Vec<String> = location
        .as_ref()
        .and_then(Url::path_segments)
        .map(|segments| segments.map(str::to_owned).collect())
        .unwrap_or_default();
    let inbox = response.status() == reqwest::StatusCode::SEE_OTHER
        && segments.get(1).map(String::as_str) == Some("exchanges");
    match segments.first() {
        Some(name) if inbox && name.len() > 1 && name.starts_with('@') => Ok(name.clone()),
        _ => Err(Problem::NotTransom),
    }
}

#[cfg(test)]
mod tests;
