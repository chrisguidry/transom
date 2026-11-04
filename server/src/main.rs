//! transom lets a program ask a person a question and get the answer back.

mod app;
mod exchanges;
mod health;
mod identities;
mod mcp;
mod push;
mod settings;
mod shutdown;
mod signin;
mod store;
mod web;
mod webhooks;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

use crate::app::App;
use crate::settings::Settings;
use crate::signin::Issuers;
use crate::store::Store;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the server.
    Serve {
        /// A YAML file of settings. `TRANSOM_` environment variables override
        /// it, and can stand in for it entirely.
        #[arg(long, env = "TRANSOM_CONFIG")]
        config: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let stderr = std::io::stderr();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with_ansi(stderr.is_terminal())
        .with_writer(std::io::stderr)
        .init();

    let Cli { command } = Cli::parse();
    let result = match command {
        Command::Serve { config } => serve(config).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "transom stopped");
            ExitCode::FAILURE
        }
    }
}

async fn serve(config: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::load(config.as_deref())?;
    let store = Store::new(
        redis::Client::open(settings.redis_url)?,
        settings.redis_prefix,
    );
    let issuers = Issuers::new(settings.issuers.into_values().collect())?;
    let app = Arc::new(App::new(store, issuers).reached_at(settings.public_url));
    tokio::spawn(exchanges::expire(app.clone()));
    let signals = shutdown::Signals::register()?;
    let listener = tokio::net::TcpListener::bind(settings.listen).await?;
    tracing::info!(address = %listener.local_addr()?, "listening");
    axum::serve(listener, app::router(app))
        .with_graceful_shutdown(signals.received())
        .await?;
    Ok(())
}
