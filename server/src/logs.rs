//! transom's logs: one line for each request it answers, with the caller
//! and, for a failure, the reason the response gave.
//!
//! A line holds the method, the path, the route it matched, the status,
//! and how long the answer took. It never holds the query, which carries
//! the sign-in callback's code, or any header or body.

use std::io::IsTerminal;
use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument;
use tracing::field::Empty;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

/// Writes logs to stderr at `info` and above, or as `RUST_LOG` says.
pub(crate) fn init() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with_ansi(std::io::stderr().is_terminal())
        .with_writer(std::io::stderr)
        .init();
}

/// Why a request failed, as its response told the caller. A `Failure`
/// leaves it on the response so the request's line can say it too.
#[derive(Clone)]
pub(crate) struct Reason(pub(crate) String);

/// Logs each request when its response is ready. A waiting request is
/// logged when the wait ends, and an event stream when it starts.
///
/// The request runs inside a `request` span, so whatever else it logs is
/// marked with its path, and signing in records the caller on that span.
pub(crate) async fn requests(request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_owned());
    let span = tracing::info_span!(
        "request",
        method = %request.method(),
        path = %request.uri().path(),
        route = route.as_deref().map(tracing::field::display),
        caller = Empty,
        issuer = Empty,
        subject = Empty,
    );
    let started = Instant::now();
    let response = next.run(request).instrument(span.clone()).await;
    let _entered = span.enter();
    let status = response.status().as_u16();
    let ms = started.elapsed().as_millis();
    let reason = response
        .extensions()
        .get::<Reason>()
        .map(|reason| reason.0.as_str());
    // Probes ask for /healthz every few seconds, and would bury the rest.
    if route.as_deref() == Some("/healthz") {
        tracing::debug!(status, ms, reason, "answered");
    } else if response.status().is_server_error() {
        tracing::warn!(status, ms, reason, "answered");
    } else {
        tracing::info!(status, ms, reason, "answered");
    }
    response
}

#[cfg(test)]
pub(crate) mod testing {
    use std::cell::Cell;
    use std::sync::{Mutex, Once};

    use tracing_subscriber::fmt::MakeWriter;
    use tracing_subscriber::fmt::writer::OptionalWriter;

    thread_local! {
        /// Where this thread's log lines go while a test captures them.
        static CAPTURING: Cell<Option<&'static Mutex<Vec<u8>>>> = const { Cell::new(None) };
    }

    /// The log lines written on this thread while it lives, at every level.
    /// A test runs on one thread unless it asks for more, so its lines are
    /// its own.
    ///
    /// One subscriber serves every test, and sorts lines by thread. A
    /// subscriber per test would race: tracing caches whether anyone wants
    /// each log statement, and tests in parallel installing and dropping
    /// their own subscribers can leave that cache saying no one does.
    pub(crate) struct Captured {
        written: &'static Mutex<Vec<u8>>,
    }

    impl Captured {
        pub(crate) fn start() -> Self {
            static INSTALLED: Once = Once::new();
            INSTALLED.call_once(|| {
                let subscriber = tracing_subscriber::fmt()
                    .with_max_level(tracing::Level::DEBUG)
                    .with_ansi(false)
                    .without_time()
                    .with_writer(|| match CAPTURING.get() {
                        Some(written) => OptionalWriter::some(written.make_writer()),
                        None => OptionalWriter::none(),
                    })
                    .finish();
                tracing::subscriber::set_global_default(subscriber).unwrap();
            });
            // The subscriber outlives every test, so what it writes to must
            // too. Each capture is a few lines, and leaks with its test.
            let written = Box::leak(Box::new(Mutex::new(Vec::new())));
            CAPTURING.set(Some(written));
            Self { written }
        }

        /// The lines that contain `text`.
        pub(crate) fn lines(&self, text: &str) -> Vec<String> {
            self.all()
                .lines()
                .filter(|line| line.contains(text))
                .map(str::to_owned)
                .collect()
        }

        /// Everything written so far.
        pub(crate) fn all(&self) -> String {
            String::from_utf8(self.written.lock().unwrap().clone()).unwrap()
        }
    }

    impl Drop for Captured {
        fn drop(&mut self) {
            CAPTURING.set(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::testing::Captured;
    use crate::app::testing::{TestApp, send};
    use crate::store::testing::store_at;

    /// The one line logged for the request that was answered.
    fn answered(logs: &Captured) -> String {
        let lines = logs.lines("answered");
        assert_eq!(lines.len(), 1, "{}", logs.all());
        lines[0].clone()
    }

    #[tokio::test]
    async fn a_failure_logs_its_route_and_its_reason() {
        let app = TestApp::new().await;
        let logs = Captured::start();
        send(&app, "GET", "/@chris/exchanges", None, None).await;
        let line = answered(&logs);
        assert!(
            line.starts_with(
                " INFO request{method=GET path=/@chris/exchanges route=/{target}/exchanges}"
            ),
            "{line}"
        );
        assert!(line.contains("status=401"), "{line}");
        assert!(
            line.contains("reason=\"sign in with a bearer token\""),
            "{line}"
        );
    }

    #[tokio::test]
    async fn a_caller_is_logged_by_name() {
        let app = TestApp::new().await;
        let token = app.issuer.token_named("u-1", "chris");
        let logs = Captured::start();
        send(&app, "GET", "/@chris", Some(&token), None).await;
        let line = answered(&logs);
        assert!(line.contains("caller=\"chris\""), "{line}");
    }

    #[tokio::test]
    async fn a_caller_without_a_name_is_logged_by_issuer_and_subject() {
        let app = TestApp::new().await;
        let token = app.issuer.token("u-1");
        let logs = Captured::start();
        send(&app, "GET", "/@chris", Some(&token), None).await;
        let line = answered(&logs);
        let issuer = app.issuer.settings().url;
        assert!(
            line.contains(&format!("issuer=\"{issuer}\" subject=\"u-1\"")),
            "{line}"
        );
    }

    #[tokio::test]
    async fn a_query_is_never_logged() {
        let app = TestApp::new().await;
        let logs = Captured::start();
        send(
            &app,
            "GET",
            "/login/callback?code=secret&state=x",
            None,
            None,
        )
        .await;
        let line = answered(&logs);
        assert!(line.contains("path=/login/callback "), "{line}");
        assert!(!logs.all().contains("secret"), "{}", logs.all());
    }

    #[tokio::test]
    async fn a_server_error_is_a_warning() {
        // Nothing listens on port 1, so every use of Redis fails.
        let app = TestApp::with_store(store_at("redis://127.0.0.1:1")).await;
        let token = app.issuer.token_named("u-1", "chris");
        let logs = Captured::start();
        send(&app, "PUT", "/@chris", Some(&token), Some(json!({}))).await;
        let line = answered(&logs);
        assert!(line.starts_with(" WARN request{"), "{line}");
        assert!(line.contains("status=503"), "{line}");
    }

    #[tokio::test]
    async fn health_checks_are_logged_only_for_debugging() {
        let app = TestApp::new().await;
        let logs = Captured::start();
        send(&app, "GET", "/healthz", None, None).await;
        let line = answered(&logs);
        assert!(line.starts_with("DEBUG request{"), "{line}");
    }

    #[tokio::test]
    async fn a_path_no_route_matches_is_logged_without_one() {
        let app = TestApp::new().await;
        let logs = Captured::start();
        send(&app, "GET", "/nowhere/at/all", None, None).await;
        let line = answered(&logs);
        assert!(
            line.contains("request{method=GET path=/nowhere/at/all}"),
            "{line}"
        );
    }
}
