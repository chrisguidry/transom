//! A real transom for a test, with a real `test-issuer` it trusts and
//! the Redis of `test-redis`, so a test talks to the server's own API
//! with nothing standing in for it.
//!
//! Each test starts a server of its own, on a port the system chooses.
//! transom's Redis prefix is a setting of the server, so a server for
//! each test gives each test keys no other test sees, and every test can
//! be `@bob` from his first request.
//!
//! `requests` acts as the people around the app, and `browser` does a
//! browser's part of a sign-in.

mod browser;
mod requests;

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use base64::Engine;
use serde_json::json;
use test_issuer::{AUDIENCE, BROWSER_CLIENT, TestIssuer};
use url::Url;

use crate::exchange::Exchange;
use crate::sync::client::Client;
use crate::sync::feed::{Event, Feed};

/// The issuer's name in transom's settings, which the sign-in page links
/// to.
const ISSUER: &str = "test";

/// A square PNG: the server's own picture of `@transom`.
pub const PICTURE: &[u8] = include_bytes!("../../../server/src/identities/transom.png");

/// An address where no server listens, for a client that cannot reach
/// its server.
pub const NOWHERE: &str = "http://127.0.0.1:1/";

/// transom's binary. Cargo builds it beside the directory of this test
/// binary whenever it builds the workspace's tests, since the server's
/// own tests run it.
fn binary() -> PathBuf {
    let tests = std::env::current_exe().unwrap();
    tests.parent().unwrap().parent().unwrap().join("transom")
}

/// A running transom, killed when it is dropped.
struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Someone with an account on the server: a handle with its `@`, and a
/// token the server takes as theirs.
#[derive(Clone, Debug)]
pub struct Person {
    pub name: String,
    pub token: String,
}

pub struct TestServer {
    /// The server's base URL, such as `http://127.0.0.1:41623/`.
    pub url: Url,
    issuer: TestIssuer,
    /// transom's Redis prefix, the same across a `restart`, so the server
    /// keeps its keys.
    prefix: String,
    /// More of transom's settings than the fixture's own.
    settings: Vec<(String, String)>,
    process: Option<Process>,
    http: reqwest::Client,
}

impl TestServer {
    /// Starts an issuer and a transom that trusts it, and returns once
    /// the server answers.
    pub async fn start() -> Self {
        Self::with(&[]).await
    }

    /// As `start`, with more of transom's settings, as `TRANSOM_`
    /// environment variables.
    pub async fn with(settings: &[(&str, &str)]) -> Self {
        static STARTED: AtomicU32 = AtomicU32::new(0);
        // The app's HTTP client is built without a TLS provider of its
        // own, and `main` installs this one before anything else runs.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let (test, process) = (STARTED.fetch_add(1, Ordering::Relaxed), std::process::id());
        let issuer = TestIssuer::start().await;
        let prefix = format!("desktop-test-{process}-{test}");
        let owned = |(name, value): &(&str, &str)| ((*name).to_owned(), (*value).to_owned());
        let settings: Vec<(String, String)> = settings.iter().map(owned).collect();
        let (process, url) = transom(&issuer.url, &prefix, "127.0.0.1:0", &settings).await;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let server = Self {
            url,
            issuer,
            prefix,
            settings,
            process: Some(process),
            http,
        };
        // The health check answers whether transom reaches Redis, which
        // it connects to only when a request needs it.
        let health = server.http.get(server.at("/healthz")).send().await.unwrap();
        assert_eq!(health.status(), 200, "transom cannot reach its Redis");
        server
    }

    /// Kills transom, which drops every connection to it as a lost
    /// network would.
    pub fn stop(&mut self) {
        self.process = None;
    }

    /// Starts transom again at the same `url` with the same keys, so a
    /// client that reconnects finds what it left.
    pub async fn restart(&mut self) {
        self.stop();
        let listen = format!("127.0.0.1:{}", self.url.port().unwrap());
        let (issuer, prefix) = (&self.issuer.url, &self.prefix);
        let (process, _) = transom(issuer, prefix, &listen, &self.settings).await;
        self.process = Some(process);
    }

    /// `path`, which begins with `/`, as a URL on the server.
    fn at(&self, path: &str) -> String {
        format!("{}{}", self.url.as_str().trim_end_matches('/'), path)
    }

    /// The app's own client, signed in as `person`.
    pub fn client(&self, person: &Person) -> Client {
        Client::new(&self.url, &person.token)
    }

    /// The issuer's URL, where something that is not transom answers.
    pub fn issuer(&self) -> Url {
        Url::parse(&self.issuer.url).unwrap()
    }
}

/// A server where `@alice` may send to `@bob`, whom the app is signed in
/// as.
pub struct Pair {
    pub server: TestServer,
    pub alice: Person,
    pub bob: Person,
}

impl Pair {
    pub async fn start() -> Self {
        let server = TestServer::start().await;
        let (alice, bob) = (server.person("alice").await, server.person("bob").await);
        server.acquaint(&alice, &bob).await;
        Self { server, alice, bob }
    }

    /// The app's own client, signed in as `@bob`.
    pub fn client(&self) -> Client {
        self.server.client(&self.bob)
    }

    /// Has `@alice` tell `@bob` `summary`, and returns the exchange's id.
    pub async fn tell(&self, summary: &str) -> String {
        let message = json!({ "summary": summary });
        self.server.send(&self.alice, "@bob", message).await
    }

    /// Has `@alice` send `@bob` a message with `PICTURE` attached as
    /// `name`, and returns the exchange's id.
    pub async fn attach(&self, name: &str) -> String {
        let data = base64::engine::general_purpose::STANDARD.encode(PICTURE);
        let attachments = json!([{ "name": name, "data": data }]);
        let message = json!({ "summary": "Disk", "attachments": attachments });
        self.server.send(&self.alice, "@bob", message).await
    }

    /// Gives `@alice` `PICTURE` as her picture, has her tell `@bob`
    /// `summary`, and returns the picture's URL as the exchange gives it.
    pub async fn pictured(&self, summary: &str) -> String {
        self.server.portray(&self.alice, PICTURE).await;
        let id = self.tell(summary).await;
        let exchange = self.server.exchange(&self.bob, &id).await;
        exchange["from_picture"].as_str().unwrap().to_owned()
    }

    /// Has `@alice` ask `@bob` `summary`, with `choices` to answer with,
    /// and returns the exchange's id.
    pub async fn ask(&self, summary: &str, choices: &[&str]) -> String {
        let message = json!({ "summary": summary, "choices": choices });
        self.server.send(&self.alice, "@bob", message).await
    }
}

/// Another of `@bob`'s devices, which hears what the server announces to
/// him from when it was made.
pub struct Elsewhere {
    feed: Feed,
}

impl Pair {
    pub async fn elsewhere(&self) -> Elsewhere {
        let feed = Feed::open(&self.client(), "@bob").await.unwrap();
        Elsewhere { feed }
    }
}

impl Elsewhere {
    /// The first exchange the server announces that `reached` is true
    /// of. It fails the test when none is announced in ten seconds.
    pub async fn hears(&mut self, reached: impl Fn(&Exchange) -> bool) -> Exchange {
        let hears = async {
            loop {
                if let Some(Event::Changed(exchange)) = self.feed.next().await.unwrap()
                    && reached(&exchange)
                {
                    return *exchange;
                }
            }
        };
        let hears = tokio::time::timeout(Duration::from_secs(10), hears);
        hears.await.expect("the server announces the change")
    }
}

/// Starts transom listening on `listen`, with `issuer` as its one issuer,
/// `prefix` before each of its keys in Redis, and `settings` over those,
/// and returns it with the URL it reported in its log.
async fn transom(
    issuer: &str,
    prefix: &str,
    listen: &str,
    settings: &[(String, String)],
) -> (Process, Url) {
    let mut command = Command::new(binary());
    // None of this process's own TRANSOM_ settings reach the server, so a
    // developer's settings cannot change it.
    let inherited = std::env::vars_os();
    let inherited = inherited.filter(|(name, _)| !name.to_string_lossy().starts_with("TRANSOM_"));
    command.env_clear().envs(inherited);
    let issuer_setting = |name: &str| format!("TRANSOM_ISSUERS__{}__{name}", ISSUER.to_uppercase());
    let child = command
        .arg("serve")
        .env("NO_COLOR", "1")
        .env("RUST_LOG", "info")
        .env("TRANSOM_LISTEN", listen)
        .env("TRANSOM_REDIS_URL", test_redis::url())
        .env("TRANSOM_REDIS_PREFIX", prefix)
        .env(issuer_setting("URL"), issuer)
        .env(issuer_setting("AUDIENCE"), AUDIENCE)
        .env(issuer_setting("NAME_CLAIM"), "preferred_username")
        .env(issuer_setting("CLIENT_ID"), BROWSER_CLIENT)
        .env(issuer_setting("LABEL"), "Test issuer")
        .envs(settings.iter().map(|(name, value)| (name, value)))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("transom's binary is built: cargo build -p transom");
    let mut process = Process(child);
    let stderr = process.0.stderr.take().unwrap();
    // The log is read on a thread of its own, so the issuer, which this
    // test's runtime serves, answers transom while it starts.
    let address = tokio::task::spawn_blocking(move || {
        let mut lines = BufReader::new(stderr).lines();
        let address = lines
            .by_ref()
            .map(Result::unwrap)
            .find_map(|line| {
                line.split_once("address=")
                    .map(|(_, rest)| rest.split_whitespace().next().unwrap().to_owned())
            })
            .expect("transom logs the address it listens on");
        // The rest of the log is read and dropped, so the server's writes
        // to it keep succeeding while it answers requests.
        std::thread::spawn(move || lines.for_each(drop));
        address
    })
    .await
    .unwrap();
    let url = Url::parse(&format!("http://{address}/")).unwrap();
    (process, url)
}
