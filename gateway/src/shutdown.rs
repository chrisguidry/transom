//! Stopping cleanly when the process is asked to.

use tokio::signal::unix::{Signal, SignalKind, signal};

/// SIGINT and SIGTERM, registered when this is created. Kubernetes stops a
/// pod with SIGTERM and a person at a terminal stops it with SIGINT; either
/// one lets in-flight requests finish before the server exits. Registering
/// before the server announces it is listening means a signal sent the
/// moment it is ready still stops it cleanly instead of killing it.
pub(crate) struct Signals {
    interrupt: Signal,
    terminate: Signal,
}

impl Signals {
    pub(crate) fn register() -> std::io::Result<Self> {
        Ok(Self {
            interrupt: signal(SignalKind::interrupt())?,
            terminate: signal(SignalKind::terminate())?,
        })
    }

    pub(crate) async fn received(mut self) {
        tokio::select! {
            _ = self.interrupt.recv() => {}
            _ = self.terminate.recv() => {}
        }
        tracing::info!("shutting down");
    }
}
