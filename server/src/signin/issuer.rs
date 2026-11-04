//! One trusted issuer: finding its signing keys and verifying its tokens.

use std::time::{Duration, Instant};

use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::sync::RwLock;

use crate::settings::IssuerSettings;

/// How long a fetch of an issuer's discovery document or keys may take.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// The least time between two fetches of an issuer's keys. A token naming a
/// key transom has not seen makes it fetch the keys again, in case the
/// issuer rotated them; without a limit, a stream of tokens with made-up
/// key ids would turn transom into a load generator against the issuer.
const REFRESH_AFTER: Duration = Duration::from_secs(30);

/// Why a token was not accepted.
#[derive(Debug)]
pub(crate) enum Rejection {
    /// The token is not one transom accepts.
    Invalid(String),
    /// transom could not reach the issuer to check the token.
    Unavailable(String),
}

fn invalid(error: impl std::fmt::Display) -> Rejection {
    Rejection::Invalid(error.to_string())
}

fn unavailable(error: impl std::fmt::Display) -> Rejection {
    Rejection::Unavailable(error.to_string())
}

/// The claims transom reads from a verified token.
#[derive(Debug, Deserialize)]
pub(crate) struct Claims {
    pub(crate) sub: String,
    #[serde(flatten)]
    pub(crate) rest: serde_json::Map<String, serde_json::Value>,
}

pub(crate) struct Issuer {
    pub(crate) settings: IssuerSettings,
    http: reqwest::Client,
    keys: RwLock<Keys>,
    refresh_after: Duration,
}

#[derive(Default)]
struct Keys {
    set: JwkSet,
    fetched: Option<Instant>,
    endpoints: Option<Endpoints>,
}

/// Where a browser signs in, and where the code it brings back is traded
/// for tokens.
#[derive(Clone, Debug)]
pub(crate) struct Endpoints {
    pub(crate) authorization: String,
    pub(crate) token: String,
}

/// The one field of an issuer's discovery document that transom needs,
/// and the issuer it names, which must be the one transom asked.
#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    jwks_uri: String,
    authorization_endpoint: Option<String>,
    token_endpoint: Option<String>,
}

impl Issuer {
    pub(crate) fn new(settings: IssuerSettings) -> Result<Self, Box<dyn std::error::Error>> {
        // reqwest's TLS needs a process-wide crypto provider. Installing it
        // a second time fails harmlessly, so every issuer can try.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let mut http = reqwest::Client::builder().timeout(FETCH_TIMEOUT);
        if let Some(path) = &settings.ca_file {
            let pem = std::fs::read(path)?;
            http = http.add_root_certificate(reqwest::Certificate::from_pem(&pem)?);
        }
        Ok(Self {
            settings,
            http: http.build()?,
            keys: RwLock::new(Keys::default()),
            refresh_after: REFRESH_AFTER,
        })
    }

    #[cfg(test)]
    pub(crate) fn refreshing_after(mut self, refresh_after: Duration) -> Self {
        self.refresh_after = refresh_after;
        self
    }

    pub(crate) async fn verify(&self, token: &str) -> Result<Claims, Rejection> {
        self.verify_for(token, &self.settings.audience).await
    }

    /// Verifies a token issued for `audience`, such as an ID token, which
    /// is issued for the client that asked for it.
    pub(crate) async fn verify_for(
        &self,
        token: &str,
        audience: &str,
    ) -> Result<Claims, Rejection> {
        let header = decode_header(token).map_err(invalid)?;
        // An issuer's keys are public, so a token signed with a shared
        // secret could have been signed by anyone holding the public key.
        if matches!(
            header.alg,
            Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512
        ) {
            return Err(invalid(
                "tokens signed with a shared secret are not accepted",
            ));
        }
        let key = self.key(header.kid.as_deref()).await?;
        let mut validation = Validation::new(header.alg);
        validation.set_audience(&[audience]);
        validation.set_issuer(&[&self.settings.url]);
        validation.set_required_spec_claims(&["aud", "exp", "iss", "sub"]);
        decode::<Claims>(token, &key, &validation)
            .map(|data| data.claims)
            .map_err(invalid)
    }

    async fn key(&self, kid: Option<&str>) -> Result<DecodingKey, Rejection> {
        if let Some(jwk) = find(&self.keys.read().await.set, kid) {
            return DecodingKey::from_jwk(jwk).map_err(invalid);
        }
        let mut keys = self.keys.write().await;
        let stale = keys
            .fetched
            .is_none_or(|fetched| fetched.elapsed() >= self.refresh_after);
        if stale {
            let (set, endpoints) = self.fetch().await?;
            keys.set = set;
            keys.endpoints = endpoints;
            keys.fetched = Some(Instant::now());
        }
        let jwk =
            find(&keys.set, kid).ok_or_else(|| invalid("no key of the issuer's signed it"))?;
        DecodingKey::from_jwk(jwk).map_err(invalid)
    }

    async fn fetch(&self) -> Result<(JwkSet, Option<Endpoints>), Rejection> {
        let url = format!(
            "{}/.well-known/openid-configuration",
            self.settings.url.trim_end_matches('/')
        );
        let discovery: Discovery = self.get(&url).await?;
        if discovery.issuer != self.settings.url {
            return Err(unavailable(format!(
                "{url} names the issuer {}",
                discovery.issuer
            )));
        }
        let endpoints = discovery
            .authorization_endpoint
            .zip(discovery.token_endpoint)
            .map(|(authorization, token)| Endpoints {
                authorization,
                token,
            });
        Ok((self.get(&discovery.jwks_uri).await?, endpoints))
    }

    /// The issuer's browser sign-in endpoints, from its discovery document.
    pub(crate) async fn endpoints(&self) -> Result<Endpoints, Rejection> {
        let mut keys = self.keys.write().await;
        if keys.endpoints.is_none() {
            let (set, endpoints) = self.fetch().await?;
            keys.set = set;
            keys.endpoints = endpoints;
            keys.fetched = Some(Instant::now());
        }
        keys.endpoints.clone().ok_or_else(|| {
            unavailable("the issuer publishes no endpoints for signing in a browser")
        })
    }

    /// POSTs `form` to the issuer's token endpoint, as its client.
    pub(crate) async fn token_request(
        &self,
        form: &[(&str, &str)],
    ) -> Result<serde_json::Value, Rejection> {
        let endpoints = self.endpoints().await?;
        let client_id = self.settings.client_id.as_deref().unwrap_or_default();
        let mut form = form.to_vec();
        let mut request = self.http.post(&endpoints.token);
        match &self.settings.client_secret {
            Some(secret) => request = request.basic_auth(client_id, Some(secret)),
            None => form.push(("client_id", client_id)),
        }
        let request = request.form(&form);
        let response = request.send().await.map_err(unavailable)?;
        if response.status().is_client_error() {
            return Err(invalid(format!(
                "the issuer refused: {}",
                response.status()
            )));
        }
        response
            .error_for_status()
            .map_err(unavailable)?
            .json()
            .await
            .map_err(unavailable)
    }

    async fn get<T: DeserializeOwned>(&self, url: &str) -> Result<T, Rejection> {
        let mut request = self.http.get(url);
        if let Some(path) = &self.settings.token_file {
            let token = tokio::fs::read_to_string(path).await.map_err(unavailable)?;
            request = request.bearer_auth(token.trim());
        }
        request
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(unavailable)?
            .json()
            .await
            .map_err(unavailable)
    }
}

/// The key a token names, or the only key when the token names none.
fn find<'a>(set: &'a JwkSet, kid: Option<&str>) -> Option<&'a Jwk> {
    match kid {
        Some(kid) => set.find(kid),
        None if set.keys.len() == 1 => set.keys.first(),
        None => None,
    }
}
