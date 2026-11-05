//! Which web UI this server serves, so that a page left open across a
//! deploy can tell that the server behind it now serves another, and load
//! itself again.

use std::fmt::Write;
use std::sync::{Arc, LazyLock};

use aws_lc_rs::digest::{Context, SHA256};
use axum::Router;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use serde_json::json;

use crate::app::App;

pub(crate) fn router() -> Router<Arc<App>> {
    Router::new().route("/version", get(serve))
}

/// Every file a page is built from or loads, by its path under `server/`'s
/// `templates/` or `server/src/web/`, with its contents.
const SOURCES: [(&str, &str); 15] = [
    (
        "static/htmx-2.0.11.min.js",
        include_str!("static/htmx-2.0.11.min.js"),
    ),
    ("static/htmx-LICENSE", include_str!("static/htmx-LICENSE")),
    (
        "static/htmx-ext-sse-2.2.4.min.js",
        include_str!("static/htmx-ext-sse-2.2.4.min.js"),
    ),
    ("static/sw.js", include_str!("static/sw.js")),
    ("static/transom.css", include_str!("static/transom.css")),
    ("static/transom.js", include_str!("static/transom.js")),
    ("static/transom.svg", include_str!("static/transom.svg")),
    (
        "static/transom.webmanifest",
        include_str!("static/transom.webmanifest"),
    ),
    (
        "templates/base.html",
        include_str!("../../templates/base.html"),
    ),
    (
        "templates/card.html",
        include_str!("../../templates/card.html"),
    ),
    (
        "templates/devices.html",
        include_str!("../../templates/devices.html"),
    ),
    (
        "templates/exchange.html",
        include_str!("../../templates/exchange.html"),
    ),
    (
        "templates/inbox.html",
        include_str!("../../templates/inbox.html"),
    ),
    (
        "templates/login.html",
        include_str!("../../templates/login.html"),
    ),
    (
        "templates/members.html",
        include_str!("../../templates/members.html"),
    ),
];

/// The first 8 bytes of a SHA-256 of each file's name, length, and
/// contents, in hex. The lengths keep where one file ends and the next
/// begins part of the hash.
fn hash(sources: &[(&str, &str)]) -> String {
    let mut context = Context::new(&SHA256);
    for (name, contents) in sources {
        for part in [name, contents] {
            context.update(&(part.len() as u64).to_be_bytes());
            context.update(part.as_bytes());
        }
    }
    let mut hex = String::new();
    for byte in &context.finish().as_ref()[..8] {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

static VERSION: LazyLock<String> = LazyLock::new(|| hash(&SOURCES));

/// The web UI's version: a hash of every file a page is built from or
/// loads. It changes exactly when what a browser receives changes, so a
/// deploy that changes only the server's other code leaves open pages
/// alone.
pub(crate) fn version() -> &'static str {
    &VERSION
}

static STARTED: LazyLock<i64> = LazyLock::new(|| jiff::Timestamp::now().as_millisecond());

/// When this process started, in milliseconds since the Unix epoch.
/// Versions are hashes, which have no order, and during a rolling deploy
/// a page hears from servers of both versions. A server that started
/// after the one that rendered the page is the newer, so a page loads
/// itself again only for a version from a later server, and never back
/// to the version it left. `App::new` reads it first, so it is the time
/// the process started rather than the time of its first page.
pub(crate) fn started() -> i64 {
    *STARTED
}

/// The version and start time, for a page to compare with its own. It
/// needs no sign-in, since the sign-in page compares them too, and says
/// nothing a page does not already carry.
async fn serve() -> Response {
    (
        [(CACHE_CONTROL, "no-store")],
        Json(json!({ "version": version(), "started": started() })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use axum::http::StatusCode;
    use serde_json::{Value, json};

    use super::{SOURCES, hash, started, version};
    use crate::app::testing::{TestApp, request};

    /// The files in one of the crate's directories, named as `SOURCES`
    /// names them.
    fn files_in(directory: &str, prefix: &str) -> BTreeSet<String> {
        std::fs::read_dir(format!("{}/{directory}", env!("CARGO_MANIFEST_DIR")))
            .unwrap()
            .map(|entry| format!("{prefix}{}", entry.unwrap().file_name().display()))
            .collect()
    }

    #[test]
    fn the_version_covers_every_template_and_static_file() {
        let mut files = files_in("templates", "templates/");
        files.extend(files_in("src/web/static", "static/"));
        let hashed: BTreeSet<String> = SOURCES.iter().map(|(name, _)| (*name).to_owned()).collect();
        assert_eq!(hashed, files);
    }

    #[test]
    fn the_version_changes_with_any_file() {
        let base = hash(&[("a", "one"), ("b", "two")]);
        assert_eq!(hash(&[("a", "one"), ("b", "two")]), base);
        assert_ne!(hash(&[("a", "one"), ("b", "twO")]), base);
        assert_ne!(hash(&[("a", "one"), ("c", "two")]), base);
        // Where one file ends and the next begins is part of the hash.
        assert_ne!(hash(&[("a", "on"), ("b", "etwo")]), base);
    }

    #[test]
    fn the_version_is_short_hex() {
        assert_eq!(version().len(), 16);
        assert!(version().bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn serves_its_version_and_start_without_signing_in() {
        let app = TestApp::new().await;
        let (status, headers, body) = request(&app, "GET", "/version", "", &[], "").await;
        assert_eq!(
            (status, headers["cache-control"].to_str().unwrap()),
            (StatusCode::OK, "no-store")
        );
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap(),
            json!({ "version": version(), "started": started() })
        );
    }

    #[test]
    fn started_is_when_the_process_started() {
        let now = jiff::Timestamp::now().as_millisecond();
        assert!((now - 3_600_000..=now).contains(&started()));
    }

    #[tokio::test]
    async fn every_page_says_which_version_rendered_it() {
        let app = TestApp::new().await;
        let html = ("accept", "text/html");
        let (_, _, page) = request(&app, "GET", "/login", "", &[html], "").await;
        let meta = format!(
            r#"<meta name="transom-version" content="{}" data-started="{}">"#,
            version(),
            started()
        );
        assert!(page.contains(&meta), "{page}");
    }
}
