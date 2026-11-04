//! The web UI: the API's own URLs, served as HTML to a client that prefers
//! it, with htmx for the actions and the event streams for live updates.

mod markdown;
pub(crate) mod pages;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, Query, Request, State};
use axum::http::header::{ACCEPT, CACHE_CONTROL, CONTENT_TYPE, LOCATION};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;

use crate::app::{App, Failure};
use crate::identities::caller;
use crate::signin::Verified;

/// Whether the request prefers HTML: a browser loading a page, or htmx
/// fetching part of one, rather than a program asking for JSON or a stream
/// of events. htmx marks its requests with `HX-Request` and does not ask
/// for HTML by `Accept`.
pub(crate) fn wants_html(headers: &HeaderMap) -> bool {
    headers.contains_key("hx-request")
        || headers
            .get(ACCEPT)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|accept| accept.contains("text/html"))
}

/// The page's rules for what it may load and run: only transom's own
/// scripts and styles, and nothing from inline code or other sites.
const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
     img-src 'self'; connect-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'";

/// For a browser asking for a page: sends it to sign in when it is not
/// signed in, and gives every page the headers that keep it to itself.
pub(crate) async fn pages_layer(request: Request, next: Next) -> Response {
    if !wants_html(request.headers()) {
        return next.run(request).await;
    }
    let path = request.uri().path().to_owned();
    let mut response = next.run(request).await;
    if response.status() == StatusCode::UNAUTHORIZED {
        let next = url::form_urlencoded::byte_serialize(path.as_bytes()).collect::<String>();
        return (
            StatusCode::SEE_OTHER,
            [(LOCATION, format!("/login?next={next}"))],
        )
            .into_response();
    }
    let headers = response.headers_mut();
    for (name, value) in [
        ("content-security-policy", CONTENT_SECURITY_POLICY),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "same-origin"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    response
}

pub(crate) fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/", get(home))
        .route("/login", get(login))
        .route("/static/{file}", get(asset))
}

/// Home is the signed-in person's inbox.
async fn home(State(app): State<Arc<App>>, verified: Verified) -> Result<Response, Failure> {
    let me = caller(&app, &verified).await?;
    Ok((
        StatusCode::SEE_OTHER,
        [(LOCATION, format!("/@{}/exchanges", me.name))],
    )
        .into_response())
}

#[derive(Deserialize)]
struct ReturnTo {
    next: Option<String>,
}

async fn login(
    State(app): State<Arc<App>>,
    Query(next): Query<ReturnTo>,
) -> Result<Response, Failure> {
    let issuers = app
        .issuers
        .for_browsers()
        .into_iter()
        .map(|issuer| {
            let settings = &issuer.settings;
            (
                settings.name.clone(),
                settings.label.clone().unwrap_or(settings.name.clone()),
            )
        })
        .collect();
    pages::login(issuers, next.next.as_deref().unwrap_or("/"))
}

/// The static files, built into the binary. Each third-party file's name
/// carries its version, so a browser may keep it for a year.
async fn asset(Path(file): Path<String>) -> Response {
    let (body, content_type, cache): (&'static str, &str, &str) = match file.as_str() {
        "htmx-2.0.11.min.js" => (
            include_str!("static/htmx-2.0.11.min.js"),
            "text/javascript",
            "public, max-age=31536000, immutable",
        ),
        "htmx-ext-sse-2.2.4.min.js" => (
            include_str!("static/htmx-ext-sse-2.2.4.min.js"),
            "text/javascript",
            "public, max-age=31536000, immutable",
        ),
        "transom.js" => (
            include_str!("static/transom.js"),
            "text/javascript",
            "no-cache",
        ),
        "transom.css" => (include_str!("static/transom.css"), "text/css", "no-cache"),
        "transom.svg" => (
            include_str!("static/transom.svg"),
            "image/svg+xml",
            "no-cache",
        ),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    ([(CONTENT_TYPE, content_type), (CACHE_CONTROL, cache)], body).into_response()
}
