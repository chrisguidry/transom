//! transom's wake-up gateway: any transom server asks it to wake a phone
//! running the official app, and it sends that phone an FCM message with
//! nothing in it, so the app syncs from its own server.

mod app;
mod credentials;
mod fcm;
mod limits;
mod logs;
mod settings;
mod shutdown;
mod wake;

#[cfg(test)]
mod stand_in;
#[cfg(test)]
mod wake_tests;

use std::process::ExitCode;
use std::sync::Arc;

use crate::app::App;
use crate::credentials::Credentials;
use crate::settings::Settings;

#[tokio::main]
async fn main() -> ExitCode {
    logs::init();
    match serve().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "the gateway stopped");
            ExitCode::FAILURE
        }
    }
}

async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::load()?;
    let credentials = Credentials::read(&settings.credentials)?;
    if let Some(credentials) = &credentials {
        tracing::info!(project = credentials.project, "sending from Firebase");
    } else {
        tracing::warn!(
            path = %settings.credentials.display(),
            "no Firebase credentials, so every wake-up is refused"
        );
    }
    let app = Arc::new(App::new(credentials, &settings.fcm_url));
    let signals = shutdown::Signals::register()?;
    let listener = tokio::net::TcpListener::bind(settings.listen).await?;
    tracing::info!(address = %listener.local_addr()?, "listening");
    axum::serve(listener, app::router(app))
        .with_graceful_shutdown(signals.received())
        .await?;
    Ok(())
}
