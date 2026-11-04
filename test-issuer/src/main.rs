//! The test issuer as a process, for tests that run against a real
//! transom. It prints its URL on the first line of standard output once it
//! is listening, and serves until SIGINT or SIGTERM.

use std::net::SocketAddr;

use clap::Parser;
use test_issuer::{AUDIENCE, BROWSER_CLIENT, Client, Options, Person, TestIssuer};
use tokio::signal::unix::{SignalKind, signal};

/// A token issuer for transom's tests, which signs in whoever asks.
///
/// `GET /authorize` signs in at once and sends the browser back with a
/// code; its `subject` and `name` query parameters choose who signs in.
/// `GET /access-token?subject=...&name=...` returns an access token as
/// plain text.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// The address to listen on.
    #[arg(long, default_value = "127.0.0.1:0")]
    listen: SocketAddr,
    /// The issuer's URL, which its tokens carry and transom is configured
    /// with. Defaults to `http://` and the address it listens on.
    #[arg(long)]
    url: Option<String>,
    /// The audience of the access tokens it signs.
    #[arg(long, default_value = AUDIENCE)]
    audience: String,
    /// The client a browser signs in through.
    #[arg(long, default_value = BROWSER_CLIENT)]
    client_id: String,
    /// The client's secret. Without one, the client is taken at its word.
    #[arg(long)]
    client_secret: Option<String>,
    /// The subject of whoever signs in when the request does not say.
    #[arg(long, default_value = "tester")]
    subject: String,
    /// The name of whoever signs in when the request does not say.
    #[arg(long, default_value = "tester")]
    name: String,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    // Registered before the URL is printed, so a signal sent the moment it
    // is ready still stops it cleanly instead of killing it.
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let listener = tokio::net::TcpListener::bind(cli.listen).await?;
    let url = cli
        .url
        .unwrap_or(format!("http://{}", listener.local_addr()?));
    let options = Options {
        audience: cli.audience,
        client: Client {
            id: cli.client_id,
            secret: cli.client_secret,
        },
        person: Person {
            subject: cli.subject,
            name: cli.name,
        },
        scripted_codes: false,
    };
    let issuer = TestIssuer::new(url, options);
    println!("{}", issuer.url);
    let stopped = async move {
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
    };
    axum::serve(listener, issuer.router())
        .with_graceful_shutdown(stopped)
        .await
}
