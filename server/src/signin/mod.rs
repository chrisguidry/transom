//! Sign-in: verifying the token on a request against the trusted issuers.
//! transom holds no passwords or keys; it only checks tokens that issuers
//! it was configured to trust have signed.

mod issuer;
#[cfg(test)]
pub(crate) mod test_issuer;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use serde::Deserialize;

use crate::app::{App, Failure};
use crate::settings::IssuerSettings;
pub(crate) use issuer::Issuer;
use issuer::Rejection;

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
}

/// The one claim read before the token is verified, to choose the issuer
/// whose keys verify it.
#[derive(Deserialize)]
struct Unverified {
    iss: String,
}

impl FromRequestParts<Arc<App>> for Verified {
    type Rejection = Failure;

    async fn from_request_parts(parts: &mut Parts, app: &Arc<App>) -> Result<Self, Failure> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or_else(|| unauthorized(None, &"sign in with a bearer token"))?;
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
        Ok(Self {
            issuer: iss,
            subject: claims.sub,
            name,
        })
    }
}

/// A 401 with the `WWW-Authenticate` challenge of RFC 6750.
fn unauthorized(error: Option<&str>, reason: &dyn std::fmt::Display) -> Failure {
    let challenge = match error {
        Some(error) => format!("Bearer realm=\"transom\", error=\"{error}\""),
        None => "Bearer realm=\"transom\"".to_owned(),
    };
    Failure::new(StatusCode::UNAUTHORIZED, reason).challenging(challenge)
}
