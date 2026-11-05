//! The gateway's logs: one line for each request it answers, with the
//! device it was asked to wake and, for a failure, the reason the response
//! gave.
//!
//! A line holds the method, the path, the route it matched, the status,
//! and how long the answer took. It names a device by a fingerprint of its
//! token, never the token, which is all anyone needs to wake the phone.

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

/// Logs each request when its response is ready. The request runs inside a
/// `request` span, on which `POST /wake` records the device's fingerprint.
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
        device = Empty,
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

        /// The one line logged for the request that was answered.
        pub(crate) fn answered(&self) -> String {
            let all = self.all();
            let lines: Vec<&str> = all
                .lines()
                .filter(|line| line.contains("answered"))
                .collect();
            assert_eq!(lines.len(), 1, "{all}");
            lines[0].to_owned()
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
