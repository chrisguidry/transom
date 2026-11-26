//! transom's HTTP API as one person, with the session token the app got
//! when it signed in.

use std::time::Duration;

use reqwest::{Method, RequestBuilder, Response};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;
use url::Url;

use super::Heard;
use crate::exchange::{Exchange, Reply};

/// Why a request did not succeed.
#[derive(Debug)]
pub enum Failure {
    /// The server could not be reached, or stopped answering part way.
    Unreachable(String),
    /// transom no longer takes this session's token: it answered 401
    /// with its own challenge. `code` is the server's word for why, when
    /// it gave one.
    SignedOut { code: Option<String> },
    /// The server answered `status` with `reason`. `winner` is the reply
    /// that won when a reply came after another one.
    Refused {
        status: u16,
        reason: String,
        winner: Option<Box<Reply>>,
    },
}

impl Failure {
    /// Whether the same request may succeed later: the server cannot be
    /// reached, or it or something before it is failing for now. A 401
    /// that is not transom's own comes from something before it, such as
    /// a proxy. Any other failure would fail again, and a session that
    /// ended stays ended.
    pub fn transient(&self) -> bool {
        match self {
            Self::Unreachable(_) => true,
            Self::SignedOut { .. } => false,
            Self::Refused { status, .. } => matches!(status, 401 | 408 | 429 | 500..),
        }
    }

    /// The failure that `response`, which did not succeed, says.
    pub(super) async fn of(response: Response) -> Self {
        #[derive(Deserialize)]
        struct Refusal {
            error: String,
            #[serde(default)]
            code: Option<String>,
            #[serde(default)]
            reply: Option<Box<Reply>>,
        }
        let status = response.status().as_u16();
        let challenge = response.headers().get("WWW-Authenticate");
        let transoms = challenge
            .and_then(|challenge| challenge.to_str().ok())
            .is_some_and(|challenge| challenge.starts_with("Bearer realm=\"transom\""));
        let body = response.text().await.unwrap_or_default();
        let refusal = serde_json::from_str::<Refusal>(&body).ok();
        if status == 401 && transoms {
            let code = refusal.and_then(|refusal| refusal.code);
            return Self::SignedOut { code };
        }
        match refusal {
            Some(refusal) => Self::Refused {
                status,
                reason: refusal.error,
                winner: refusal.reply,
            },
            None => Self::Refused {
                status,
                reason: body,
                winner: None,
            },
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(cause) => write!(f, "the server could not be reached: {cause}"),
            Self::SignedOut { .. } => write!(f, "transom ended this session"),
            Self::Refused { status, reason, .. } => {
                write!(f, "transom answered {status}: {reason}")
            }
        }
    }
}

impl From<reqwest::Error> for Failure {
    fn from(error: reqwest::Error) -> Self {
        Self::Unreachable(error.without_url().to_string())
    }
}

/// How a reply turned out.
#[derive(Debug)]
pub enum Replied {
    /// The reply is the exchange's reply, and this is the exchange with
    /// it.
    Accepted(Box<Exchange>),
    /// Someone else replied first, with this: the first reply wins.
    Lost(Box<Reply>),
}

/// An identity as `GET /@{name}` describes it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Identity {
    pub name: String,
    pub kind: Kind,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub picture: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Individual,
    Group,
}

/// One of an identity's devices, and how the server wakes it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub wake: serde_json::Value,
}

impl Device {
    /// How the device hears of a message, as the devices list says it.
    pub fn wakes(&self) -> &'static str {
        if self.wake.get("web_push").is_some() {
            "Web Push"
        } else if self.wake.get("fcm").is_some() {
            "Google Play services"
        } else {
            "Direct connection"
        }
    }
}

/// The API of one server, as one signed-in person. Cloning it is cheap,
/// and every clone shares one connection pool.
#[derive(Clone, Debug)]
pub struct Client {
    base: Url,
    token: String,
    http: reqwest::Client,
    reports: Option<Reports>,
}

/// Where a client says that the server ended its session, and which of
/// the account's sessions it is, so a report from a session already
/// over is told from one about the session running now.
#[derive(Clone, Debug)]
struct Reports {
    generation: u64,
    heard: UnboundedSender<Heard>,
}

impl Client {
    pub fn new(server: &Url, token: &str) -> Self {
        Self {
            base: server.clone(),
            token: token.to_owned(),
            http: http(),
            reports: None,
        }
    }

    /// This client, which also sends `heard` each time the server says
    /// the session ended, as session `generation`. Every call reports,
    /// wherever it was made, so a call from a screen or for a picture
    /// ends the session as surely as the feed does.
    pub fn reporting(self, generation: u64, heard: UnboundedSender<Heard>) -> Self {
        Self {
            reports: Some(Reports { generation, heard }),
            ..self
        }
    }

    /// Passes `failure` back, after reporting it when it says the
    /// session ended.
    pub(super) fn report(&self, failure: Failure) -> Failure {
        if let (Failure::SignedOut { code }, Some(reports)) = (&failure, &self.reports) {
            let generation = reports.generation;
            let code = code.clone();
            let _ = reports.heard.send(Heard::SignedOut { generation, code });
        }
        failure
    }

    /// `segments` as a URL on this client's server.
    pub fn url(&self, segments: &[&str]) -> Url {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .expect("an http URL has path segments")
            .pop_if_empty()
            .extend(segments);
        url
    }

    /// A request as this person, which says nothing yet of what it
    /// accepts. The feed asks for an event stream with it: a second
    /// `Accept` header would be added to the first, not replace it.
    pub(super) fn signed(&self, method: Method, url: Url) -> RequestBuilder {
        self.http.request(method, url).bearer_auth(&self.token)
    }

    fn request(&self, method: Method, url: Url) -> RequestBuilder {
        self.signed(method, url)
            .header("Accept", "application/json")
    }

    async fn send(&self, request: RequestBuilder) -> Result<Response, Failure> {
        let response = request.timeout(Duration::from_secs(30)).send().await?;
        if response.status().is_success() {
            return Ok(response);
        }
        Err(self.report(Failure::of(response).await))
    }

    async fn json<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, Failure> {
        let response = self.send(request).await?;
        Ok(response.json().await?)
    }

    /// `name`'s exchanges, sent and received, newest first. `name` is a
    /// handle with its `@`.
    pub async fn exchanges(&self, name: &str) -> Result<Vec<Exchange>, Failure> {
        #[derive(Deserialize)]
        struct Listed {
            exchanges: Vec<Exchange>,
        }
        let listed: Listed = self
            .json(self.request(Method::GET, self.url(&[name, "exchanges"])))
            .await?;
        Ok(listed.exchanges)
    }

    /// Replies to exchange `id` with `choice`, a choice's key, and
    /// `text`, at least one of them. Repeating the same reply is safe, so
    /// a queued reply can be sent again.
    pub async fn reply(
        &self,
        id: &str,
        choice: Option<&str>,
        text: Option<&str>,
    ) -> Result<Replied, Failure> {
        let request = self
            .request(Method::PUT, self.url(&["exchanges", id, "reply"]))
            .json(&json!({ "choice": choice, "text": text }));
        match self.json(request).await {
            Ok(exchange) => Ok(Replied::Accepted(Box::new(exchange))),
            Err(Failure::Refused {
                winner: Some(winner),
                ..
            }) => Ok(Replied::Lost(winner)),
            Err(failure) => Err(failure),
        }
    }

    /// Takes back the message of exchange `id`, which only its sender
    /// may do while it is open.
    pub async fn withdraw(&self, id: &str) -> Result<(), Failure> {
        self.send(self.request(Method::DELETE, self.url(&["exchanges", id, "message"])))
            .await?;
        Ok(())
    }

    /// Marks exchange `id` read or unread for `name`, this person.
    pub async fn mark(&self, id: &str, name: &str, read: bool) -> Result<(), Failure> {
        let method = if read { Method::PUT } else { Method::DELETE };
        self.send(self.request(method, self.url(&["exchanges", id, name])))
            .await?;
        Ok(())
    }

    /// Marks each of `ids` read for `name`, this person, in one request.
    pub async fn mark_read(&self, name: &str, ids: &[String]) -> Result<(), Failure> {
        let request = self
            .request(Method::POST, self.url(&[name, "read"]))
            .json(&json!({ "exchanges": ids }));
        self.send(request).await?;
        Ok(())
    }

    /// The bytes of the picture at `url`, which the server gives as an
    /// identity's `…_picture`. Only its path and version are taken, and
    /// asked of this client's own server, so the token never goes
    /// anywhere else.
    pub async fn picture(&self, url: &str) -> Result<Vec<u8>, Failure> {
        let given = self.base.join(url).unwrap_or_else(|_| self.base.clone());
        let mut own = self.base.clone();
        own.set_path(given.path());
        own.set_query(given.query());
        let response = self.send(self.request(Method::GET, own)).await?;
        Ok(response.bytes().await?.to_vec())
    }

    /// The bytes of `name`, an image attached to exchange `id`.
    pub async fn attachment(&self, id: &str, name: &str) -> Result<Vec<u8>, Failure> {
        let url = self.url(&["exchanges", id, "attachments", name]);
        let response = self.send(self.request(Method::GET, url)).await?;
        Ok(response.bytes().await?.to_vec())
    }

    /// Registers this computer as `name`'s device `device`, which holds
    /// the live feed open. Registering again replaces it.
    pub async fn register(&self, name: &str, device: &str, called: &str) -> Result<(), Failure> {
        let request = self
            .request(Method::PUT, self.url(&[name, "devices", device]))
            .json(&json!({ "name": called, "wake": { "connection": true } }));
        self.send(request).await?;
        Ok(())
    }

    pub async fn devices(&self, name: &str) -> Result<Vec<Device>, Failure> {
        #[derive(Deserialize)]
        struct Listed {
            devices: Vec<Device>,
        }
        let listed: Listed = self
            .json(self.request(Method::GET, self.url(&[name, "devices"])))
            .await?;
        Ok(listed.devices)
    }

    /// Forgets `name`'s device `device`, and ends the session that
    /// registered it.
    pub async fn forget(&self, name: &str, device: &str) -> Result<(), Failure> {
        self.send(self.request(Method::DELETE, self.url(&[name, "devices", device])))
            .await?;
        Ok(())
    }

    pub async fn identity(&self, name: &str) -> Result<Identity, Failure> {
        self.json(self.request(Method::GET, self.url(&[name])))
            .await
    }

    /// Changes `name`'s handle or display name, and returns it as it is
    /// now.
    pub async fn change(&self, name: &str, change: serde_json::Value) -> Result<Identity, Failure> {
        self.json(self.request(Method::PATCH, self.url(&[name])).json(&change))
            .await
    }

    /// The handles of `group`'s members.
    pub async fn members(&self, group: &str) -> Result<Vec<String>, Failure> {
        #[derive(Deserialize)]
        struct Listed {
            members: Vec<String>,
        }
        let listed: Listed = self
            .json(self.request(Method::GET, self.url(&[group, "members"])))
            .await?;
        Ok(listed.members)
    }

    /// Invites `member` to `group`: `@transom` asks them first.
    pub async fn invite(&self, group: &str, member: &str) -> Result<(), Failure> {
        self.send(self.request(Method::PUT, self.url(&[group, "members", member])))
            .await?;
        Ok(())
    }

    pub async fn remove(&self, group: &str, member: &str) -> Result<(), Failure> {
        self.send(self.request(Method::DELETE, self.url(&[group, "members", member])))
            .await?;
        Ok(())
    }

    /// Ends this session on the server. The app forgets the session
    /// whatever the server answers.
    pub async fn sign_out(&self) {
        let _ = self
            .send(self.request(Method::POST, self.url(&["logout"])))
            .await;
    }
}

/// The HTTP client every request shares. It follows no redirects: the
/// server answers `GET /` with a redirect that sign-in reads, and no API
/// call needs one followed.
pub fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("transom-desktop/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("the HTTP client builds from fixed settings")
}

#[cfg(test)]
mod tests;
