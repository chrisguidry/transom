//! Sign-in: verifying the token on a request against the trusted issuers.
//! transom holds no passwords or keys; it only checks tokens that issuers
//! it was configured to trust have signed.

pub(crate) mod app;
#[cfg(test)]
mod app_tests;
pub(crate) mod browser;
#[cfg(test)]
mod browser_tests;
mod issuer;
pub(crate) mod metadata;
#[cfg(test)]
mod metadata_tests;
pub(crate) mod sessions;
#[cfg(test)]
pub(crate) mod test_issuer;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::app::{App, Failure};
use crate::settings::IssuerSettings;
pub(crate) use issuer::Issuer;
use issuer::Rejection;
use metadata::Resource;

/// The trusted issuers, by the URL their tokens carry in `iss`.
pub(crate) struct Issuers(HashMap<String, Issuer>);

impl Issuers {
    pub(crate) fn new(settings: Vec<IssuerSettings>) -> Result<Self, Box<dyn std::error::Error>> {
        let mut issuers = HashMap::new();
        for settings in settings {
            let url = settings.url.clone();
            if issuers
                .insert(url.clone(), Issuer::new(settings)?)
                .is_some()
            {
                return Err(format!("two issuers have the URL {url}").into());
            }
        }
        Ok(Self(issuers))
    }

    /// The issuer whose tokens carry `url` as their `iss`.
    pub(crate) fn by_url(&self, url: &str) -> Option<&Issuer> {
        self.0.get(url)
    }

    /// The issuer named `name` in the settings.
    pub(crate) fn named(&self, name: &str) -> Option<&Issuer> {
        self.0.values().find(|issuer| issuer.settings.name == name)
    }

    /// The issuers a person can sign in to a browser with: those with a
    /// client, in order of name.
    pub(crate) fn for_browsers(&self) -> Vec<&Issuer> {
        let mut issuers: Vec<&Issuer> = self
            .0
            .values()
            .filter(|issuer| issuer.settings.client_id.is_some())
            .collect();
        issuers.sort_by(|a, b| a.settings.name.cmp(&b.settings.name));
        issuers
    }

    #[cfg(test)]
    pub(crate) fn of(issuers: Vec<Issuer>) -> Self {
        Self(
            issuers
                .into_iter()
                .map(|issuer| (issuer.settings.url.clone(), issuer))
                .collect(),
        )
    }
}

/// The caller of a request, as its verified token describes it.
#[derive(Debug)]
pub(crate) struct Verified {
    /// The issuer's URL.
    pub(crate) issuer: String,
    pub(crate) subject: String,
    /// The value of the issuer's name claim, if it has one and the token
    /// carries it.
    pub(crate) name: Option<String>,
    /// The digest of the session the request presented as its bearer
    /// token. A device registered with it ends the session when it is
    /// forgotten, so a lost phone loses its access. A browser's cookie
    /// leaves it empty, so turning off a browser's notifications does not
    /// sign it out.
    pub(crate) session: Option<String>,
}

/// What every session token the app holds starts with. An issuer's token
/// is a JWT, whose first part is base64url JSON and so starts with `ey`,
/// so the two cannot be mistaken for each other.
pub(crate) const SESSION_PREFIX: &str = "transom_";

/// The bearer token on a request.
enum Bearer<'a> {
    /// A session's token, without its prefix.
    Session(&'a str),
    /// A token from an issuer.
    Issued(&'a str),
}

impl<'a> Bearer<'a> {
    fn of(headers: &'a HeaderMap) -> Option<Self> {
        let token = headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))?;
        Some(match token.strip_prefix(SESSION_PREFIX) {
            Some(session) => Self::Session(session),
            None => Self::Issued(token),
        })
    }
}

/// The one claim read before the token is verified, to choose the issuer
/// whose keys verify it.
#[derive(Deserialize)]
struct Unverified {
    iss: String,
}

impl FromRequestParts<Arc<App>> for Verified {
    type Rejection = Failure;

    /// Verifies the request's caller, and records it on the request's span
    /// for its log line.
    async fn from_request_parts(parts: &mut Parts, app: &Arc<App>) -> Result<Self, Failure> {
        Self::for_resource(parts, app, Resource::Api).await
    }
}

impl Verified {
    /// Verifies the request's caller to `resource`. A 401 points the
    /// client to the resource's metadata, which names the issuers it can
    /// sign a person in with.
    pub(crate) async fn for_resource(
        parts: &mut Parts,
        app: &Arc<App>,
        resource: Resource,
    ) -> Result<Self, Failure> {
        let verified = verify(parts, app).await.map_err(|failure| {
            failure.pointing_to_metadata(&resource.metadata_url(app, &parts.headers))
        })?;
        let span = tracing::Span::current();
        match &verified.name {
            Some(name) => span.record("caller", name.as_str()),
            None => span
                .record("issuer", verified.issuer.as_str())
                .record("subject", verified.subject.as_str()),
        };
        Ok(verified)
    }
}

/// Verifies the request's bearer token, or its browser session.
async fn verify(parts: &mut Parts, app: &Arc<App>) -> Result<Verified, Failure> {
    let token = match Bearer::of(&parts.headers) {
        Some(Bearer::Issued(token)) => token,
        Some(Bearer::Session(session)) => return sessions::session(app, session).await,
        None => return browser_session(parts, app).await,
    };
    let iss = jsonwebtoken::dangerous::insecure_decode_claims::<Unverified>(token)
        .map_err(|error| unauthorized(Some("invalid_token"), &error))?
        .iss;
    let issuer = app
        .issuers
        .0
        .get(&iss)
        .ok_or_else(|| unauthorized(Some("invalid_token"), &"the issuer is not trusted"))?;
    let mut claims = issuer
        .verify(token)
        .await
        .map_err(|rejection| match rejection {
            Rejection::Invalid(reason) => unauthorized(Some("invalid_token"), &reason),
            Rejection::Unavailable(reason) => {
                tracing::warn!(issuer = %iss, %reason, "an issuer's keys are unavailable");
                Failure::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "the token's issuer could not be reached",
                )
            }
        })?;
    let name = issuer
        .settings
        .name_claim
        .as_ref()
        .and_then(|claim| claims.rest.remove(claim))
        .and_then(|value| value.as_str().map(str::to_owned));
    Ok(Verified {
        issuer: iss,
        subject: claims.sub,
        name,
        session: None,
    })
}

/// Verifies the request's browser session, from its cookie.
async fn browser_session(parts: &Parts, app: &App) -> Result<Verified, Failure> {
    let Some(session) = browser::cookie(&parts.headers, browser::SESSION_COOKIE) else {
        return Err(unauthorized(None, &"sign in with a bearer token"));
    };
    // A browser sends its cookie with any request, even one another site
    // made it send. A custom header, which another site cannot add, shows
    // the request came from transom's own pages, as RFC 10017 describes;
    // htmx adds HX-Request to each of its own.
    if !parts.method.is_safe() && !parts.headers.contains_key("hx-request") {
        return Err(Failure::new(
            StatusCode::FORBIDDEN,
            "a request signed in by cookie must come from transom's own pages",
        ));
    }
    let verified = sessions::session(app, &session).await?;
    Ok(Verified {
        session: None,
        ..verified
    })
}

/// A 401 with the `WWW-Authenticate` challenge of RFC 6750, to which
/// [`Verified::for_resource`] adds where the resource's metadata is.
pub(crate) fn unauthorized(error: Option<&str>, reason: &dyn std::fmt::Display) -> Failure {
    let challenge = match error {
        Some(error) => format!("Bearer realm=\"transom\", error=\"{error}\""),
        None => "Bearer realm=\"transom\"".to_owned(),
    };
    Failure::new(StatusCode::UNAUTHORIZED, reason).challenging(challenge)
}

pub(crate) fn router() -> axum::Router<Arc<App>> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/login/callback", get(browser::finish))
        .route("/login/{issuer}", get(browser::begin))
        .route("/login/app", post(app::redeem))
        .route("/logout", post(logout))
        .merge(metadata::router())
}

/// Ends the session the request carries: the app's bearer token, answered
/// with `204`, or the browser's cookie, sent back to sign in.
async fn logout(
    axum::extract::State(app): axum::extract::State<Arc<App>>,
    headers: HeaderMap,
) -> Result<Response, Failure> {
    if let Some(Bearer::Session(session)) = Bearer::of(&headers) {
        sessions::end(&app, session).await?;
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    browser::logout(&app, &headers).await
}
