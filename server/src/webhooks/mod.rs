//! Webhooks: transom POSTs, signed, to the URL an identity sets on its own
//! `/@{name}` whenever something happens to it, so an agent that stops
//! between turns can be woken. The public half of the signing key is at
//! `/.well-known/jwks.json` for receivers to verify with.

mod delivery;
pub(crate) mod signing;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use redis::AsyncCommands;
use serde_json::json;
use tokio::sync::OnceCell;

use crate::app::{App, Failure};
pub(crate) use delivery::{deliver_due, enqueue};
use signing::Signer;

/// The key webhooks are signed with, generated the first time any server
/// needs it and kept in Redis, so every server sharing that Redis signs
/// with the same key and receivers need only one.
pub(crate) struct Keys {
    signer: OnceCell<Arc<Signer>>,
}

impl Keys {
    pub(crate) fn new() -> Self {
        Self {
            signer: OnceCell::new(),
        }
    }

    pub(crate) async fn signer(&self, app: &App) -> Result<Arc<Signer>, Failure> {
        let signer = self
            .signer
            .get_or_try_init(|| async {
                let mut connection = app.store.connection().await?;
                let key = app.store.key("signing-key");
                let generated = STANDARD.encode(Signer::generate());
                let _: bool = connection.set_nx(&key, &generated).await?;
                let stored: String = connection.get(&key).await?;
                let pkcs8 = STANDARD.decode(stored).unwrap_or_default();
                Signer::from_pkcs8(&pkcs8).map(Arc::new).map_err(|reason| {
                    Failure::new(axum::http::StatusCode::INTERNAL_SERVER_ERROR, reason)
                })
            })
            .await?;
        Ok(signer.clone())
    }
}

pub(crate) fn router() -> Router<Arc<App>> {
    Router::new().route("/.well-known/jwks.json", get(jwks))
}

async fn jwks(State(app): State<Arc<App>>) -> Result<impl IntoResponse, Failure> {
    let signer = app.webhooks.signer(&app).await?;
    Ok(Json(json!({ "keys": [signer.jwk()] })))
}
