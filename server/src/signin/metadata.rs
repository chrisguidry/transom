//! OAuth 2.0 Protected Resource Metadata (RFC 9728): the document that
//! tells a client, such as an MCP client meeting `/mcp` for the first
//! time, which issuers it can sign a person in with to call transom.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::app::App;

/// The scopes transom asks an issuer for when it signs a person in:
/// `profile` and `email` carry the claims an issuer's `name_claim`
/// usually names, such as `preferred_username` or `email`, and
/// `offline_access` gets a refresh token, so a client keeps its access
/// past the first token's expiry.
pub(crate) const SCOPES: [&str; 4] = ["openid", "profile", "email", "offline_access"];

/// How long a client may keep the document. It changes only when the
/// settings do.
const CACHE: &str = "max-age=3600";

/// A resource that transom protects, which a client finds the metadata
/// of at the well-known path followed by the resource's own path.
#[derive(Clone, Copy)]
pub(crate) enum Resource {
    /// The HTTP API, whose identifier is transom's origin.
    Api,
    /// The MCP server at `/mcp`.
    Mcp,
}

impl Resource {
    fn path(self) -> &'static str {
        match self {
            Self::Api => "",
            Self::Mcp => "/mcp",
        }
    }

    /// The resource's identifier, which a client compares to the URL it
    /// called and refuses the metadata if they differ.
    fn identifier(self, app: &App, headers: &HeaderMap) -> String {
        format!("{}{}", app.origin(headers), self.path())
    }

    /// Where the resource's metadata is, for the `resource_metadata` of a
    /// `WWW-Authenticate` challenge.
    pub(crate) fn metadata_url(self, app: &App, headers: &HeaderMap) -> String {
        format!(
            "{}/.well-known/oauth-protected-resource{}",
            app.origin(headers),
            self.path()
        )
    }
}

fn describe(app: &App, headers: &HeaderMap, resource: Resource) -> Response {
    // Only an issuer that transom has a client at can sign a person in.
    // One that only verifies tokens, like a Kubernetes cluster's, has no
    // sign-in for a client to start.
    let issuers: Vec<&str> = app
        .issuers
        .for_browsers()
        .into_iter()
        .map(|issuer| issuer.settings.url.as_str())
        .collect();
    let metadata = json!({
        "resource": resource.identifier(app, headers),
        "authorization_servers": issuers,
        "bearer_methods_supported": ["header"],
        "scopes_supported": SCOPES,
        "resource_name": "transom",
    });
    ([(CACHE_CONTROL, CACHE)], Json(metadata)).into_response()
}

async fn api(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    describe(&app, &headers, Resource::Api)
}

async fn mcp(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    describe(&app, &headers, Resource::Mcp)
}

pub(crate) fn router() -> axum::Router<Arc<App>> {
    use axum::routing::get;
    axum::Router::new()
        .route("/.well-known/oauth-protected-resource", get(api))
        .route("/.well-known/oauth-protected-resource/mcp", get(mcp))
}
