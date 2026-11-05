//! The watchdog: while transom can reach Redis, it sends a GET to a URL on
//! a schedule, so a dead man's switch such as a healthchecks.io check
//! notices when transom stops working. A process that runs but cannot reach
//! Redis cannot deliver anything, so it does not ping. The URL carries no
//! credentials of its own; whatever secret it needs is in the URL.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::MissedTickBehavior;

use crate::app::App;

/// How long one ping may take before transom gives up on it until the next.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Pings `url` now and every `interval` after, until the process stops.
/// Every replica pings; a dead man's switch only needs one of them.
pub(crate) async fn watch(app: Arc<App>, url: url::Url, interval: Duration) {
    let mut ticks = tokio::time::interval(interval);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        if let Err(error) = ping(&app, &url).await {
            tracing::warn!(%error, "the watchdog did not ping");
        }
    }
}

async fn ping(app: &App, url: &url::Url) -> Result<(), String> {
    app.store
        .ping()
        .await
        .map_err(|error| format!("Redis is unreachable: {error}"))?;
    app.http
        .get(url.clone())
        .timeout(TIMEOUT)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map(drop)
        .map_err(|error| format!("the ping failed: {error}"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use axum::http::StatusCode;
    use axum::routing::get;
    use tokio::net::TcpListener;

    use crate::app::testing::TestApp;
    use crate::store::testing::{redis_url, store_at};

    /// A dead man's switch that counts the pings it gets and answers each
    /// with `status`.
    async fn switch(status: StatusCode) -> (url::Url, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/ping/abc", listener.local_addr().unwrap());
        let pings = Arc::new(AtomicUsize::new(0));
        let counted = pings.clone();
        let router = Router::new().route(
            "/ping/abc",
            get(move || async move {
                counted.fetch_add(1, Ordering::SeqCst);
                status
            }),
        );
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (url::Url::parse(&url).unwrap(), pings)
    }

    #[tokio::test]
    async fn pings_while_redis_answers() {
        let (url, pings) = switch(StatusCode::OK).await;
        let app = TestApp::with_store(store_at(&redis_url())).await;
        assert_eq!(super::ping(&app.app, &url).await, Ok(()));
        assert_eq!(pings.load(Ordering::SeqCst), 1);
    }

    /// A port that nothing listens on: bound to find a free one, then closed.
    async fn closed_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[tokio::test]
    async fn does_not_ping_when_redis_is_unreachable() {
        let (url, pings) = switch(StatusCode::OK).await;
        let redis = format!("redis://127.0.0.1:{}", closed_port().await);
        let app = TestApp::with_store(store_at(&redis)).await;
        assert!(super::ping(&app.app, &url).await.is_err());
        assert_eq!(pings.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn reports_a_ping_the_switch_refuses() {
        let (url, pings) = switch(StatusCode::NOT_FOUND).await;
        let app = TestApp::with_store(store_at(&redis_url())).await;
        assert!(super::ping(&app.app, &url).await.is_err());
        assert_eq!(pings.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn keeps_pinging_after_a_ping_fails() {
        let (url, pings) = switch(StatusCode::SERVICE_UNAVAILABLE).await;
        let app = TestApp::with_store(store_at(&redis_url())).await;
        let watching = tokio::spawn(super::watch(
            app.app.clone(),
            url,
            std::time::Duration::from_millis(50),
        ));
        tokio::time::sleep(std::time::Duration::from_millis(220)).await;
        watching.abort();
        assert!(pings.load(Ordering::SeqCst) >= 2);
    }
}
