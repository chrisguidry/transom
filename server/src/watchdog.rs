//! The watchdog: while transom can reach Redis, it sends a GET to a URL on
//! a schedule, so a dead man's switch such as a healthchecks.io check
//! notices when transom stops working. A process that runs but cannot reach
//! Redis cannot deliver anything, so it does not ping. The URL carries no
//! credentials of its own; whatever secret it needs is in the URL.

use std::sync::Arc;
use std::time::Duration;

use crate::app::App;

/// How long one ping may take before transom gives up on it.
const TIMEOUT: Duration = Duration::from_secs(10);

/// How soon transom tries again after a ping that did not go through, such
/// as the first one after a restart, made before the pod's network is up.
const RETRY: Duration = Duration::from_secs(30);

/// Pings `url` now and every `interval` after, until the process stops.
/// Every replica pings; a dead man's switch only needs one of them.
pub(crate) fn watch(app: Arc<App>, url: url::Url, interval: Duration) -> impl Future<Output = ()> {
    watch_retrying(app, url, interval, RETRY)
}

/// `watch`, trying again `retry` after a ping that did not go through, or
/// after `interval` if that is sooner.
async fn watch_retrying(app: Arc<App>, url: url::Url, interval: Duration, retry: Duration) {
    loop {
        let wait = match ping(&app, &url).await {
            Ok(()) => interval,
            Err(error) => {
                tracing::warn!(%error, "the watchdog did not ping");
                retry.min(interval)
            }
        };
        tokio::time::sleep(wait).await;
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
    use crate::store::testing::store_at;

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
        let app = TestApp::with_store(store_at(&test_redis::url())).await;
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
        let app = TestApp::with_store(store_at(&test_redis::url())).await;
        assert!(super::ping(&app.app, &url).await.is_err());
        assert_eq!(pings.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn pings_again_after_the_interval() {
        let (url, pings) = switch(StatusCode::OK).await;
        let app = TestApp::with_store(store_at(&test_redis::url())).await;
        let watching = tokio::spawn(super::watch(
            app.app.clone(),
            url,
            std::time::Duration::from_millis(50),
        ));
        tokio::time::sleep(std::time::Duration::from_millis(220)).await;
        watching.abort();
        assert!(pings.load(Ordering::SeqCst) >= 2);
    }

    #[tokio::test]
    async fn tries_a_failed_ping_again_before_the_interval() {
        let (url, pings) = switch(StatusCode::SERVICE_UNAVAILABLE).await;
        let app = TestApp::with_store(store_at(&test_redis::url())).await;
        let watching = tokio::spawn(super::watch_retrying(
            app.app.clone(),
            url,
            std::time::Duration::from_hours(1),
            std::time::Duration::from_millis(50),
        ));
        tokio::time::sleep(std::time::Duration::from_millis(220)).await;
        watching.abort();
        assert!(pings.load(Ordering::SeqCst) >= 2);
    }
}
