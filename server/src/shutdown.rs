//! Stopping cleanly when the process is asked to.
//!
//! On a signal the server cancels [`App::stopping`](crate::app::App): the
//! maintenance pass claims no more work, and the live feeds and waiting
//! requests end, so the server's open connections close and their clients
//! reconnect to another server at once. The server then waits a few
//! seconds for the attempts the pass already started, and exits.

use std::time::Duration;

use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::task::JoinHandle;

/// How long a stopping server waits for the attempts its maintenance pass
/// already started. Kubernetes kills a pod 30 seconds after it asks it to
/// stop, unless told otherwise. An attempt still running after this is
/// abandoned, and its lease hands it to another server.
pub(crate) const FINISH_WITHIN: Duration = Duration::from_secs(5);

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

/// Waits up to `within` for the maintenance pass to finish what it
/// started, and says whether it did.
pub(crate) async fn finish(pass: JoinHandle<()>, within: Duration) -> bool {
    let finished = tokio::time::timeout(within, pass).await.is_ok();
    if !finished {
        tracing::warn!("stopped during an attempt, which another server takes over");
    }
    finished
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    async fn finished(pass: impl Future<Output = ()> + Send + 'static) -> bool {
        super::finish(tokio::spawn(pass), Duration::from_millis(100)).await
    }

    #[tokio::test]
    async fn a_pass_that_finishes_in_time_is_waited_for() {
        assert!(finished(async {}).await);
    }

    #[tokio::test]
    async fn a_pass_that_takes_too_long_is_abandoned() {
        assert!(!finished(std::future::pending()).await);
    }
}
