//! The test desktop as a process, for running the desktop app by hand or
//! from a script on a private bus, such as one `dbus-run-session`
//! starts. It prints `ready` once its services have their names, and
//! serves until SIGINT or SIGTERM.

use clap::Parser;
use test_desktop::TestDesktop;
use tokio::signal::unix::{SignalKind, signal};

/// A desktop's notification server, tray registry, and keyring, for
/// transom's tests.
///
/// It serves `org.freedesktop.Notifications`,
/// `org.kde.StatusNotifierWatcher`, and `org.freedesktop.secrets` on the
/// session bus, so run it on a private one. `sh.transom.TestDesktop` at
/// `/sh/transom/TestDesktop` lists the notifications as JSON, and
/// presses or dismisses one.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// A secret to keep in the keyring from the start, under the
    /// attribute `application=sh.transom`, as the desktop app keeps its
    /// session.
    #[arg(long)]
    secret: Option<String>,
}

#[tokio::main]
async fn main() -> zbus::Result<()> {
    let cli = Cli::parse();
    let desktop = TestDesktop::serve(zbus::Connection::session().await?).await?;
    if let Some(secret) = cli.secret {
        desktop
            .keep_secret(&[("application", "sh.transom")], secret.as_bytes())
            .await?;
    }
    // The handlers are in place before the line that says the process
    // is ready, so a signal sent as soon as the line is read stops the
    // process cleanly instead of killing it.
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    println!("ready");
    tokio::select! {
        _ = interrupt.recv() => {}
        _ = terminate.recv() => {}
    }
    Ok(())
}
