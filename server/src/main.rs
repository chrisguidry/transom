//! transom lets a program ask a person a question and get the answer back.

mod health;
mod shutdown;

use std::io::IsTerminal;
use std::net::SocketAddr;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

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
        /// The address to listen on.
        #[arg(long, env = "TRANSOM_LISTEN", default_value = "0.0.0.0:8080")]
        listen: SocketAddr,

        /// The Redis server that holds transom's data.
        #[arg(
            long,
            env = "TRANSOM_REDIS_URL",
            default_value = "redis://127.0.0.1:6379"
        )]
        redis_url: String,
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
        Command::Serve { listen, redis_url } => serve(listen, &redis_url).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "transom stopped");
            ExitCode::FAILURE
        }
    }
}

async fn serve(listen: SocketAddr, redis_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let redis = redis::Client::open(redis_url)?;
    let signals = shutdown::Signals::register()?;
    let listener = tokio::net::TcpListener::bind(listen).await?;
    tracing::info!(address = %listener.local_addr()?, "listening");
    axum::serve(listener, health::router(redis))
        .with_graceful_shutdown(signals.received())
        .await?;
    Ok(())
}
