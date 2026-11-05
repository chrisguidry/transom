//! The health check, which reports whether transom can reach Redis.

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;

use crate::app::App;

pub(crate) fn router() -> Router<Arc<App>> {
    Router::new().route("/healthz", get(healthz))
}

async fn healthz(State(app): State<Arc<App>>) -> (StatusCode, &'static str) {
    match app.store.ping().await {
        Ok(()) => (StatusCode::OK, "ok\n"),
        Err(error) => {
            tracing::warn!(%error, "Redis is unreachable");
            (StatusCode::SERVICE_UNAVAILABLE, "Redis is unreachable\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use tokio::net::TcpListener;

    use crate::app::testing::{TestApp, send};
    use crate::store::testing::store_at;

    async fn check(redis_url: &str) -> (StatusCode, String) {
        let app = TestApp::with_store(store_at(redis_url)).await;
        let (status, _, body) = send(&app, "GET", "/healthz", None, None).await;
        (status, body)
    }

    #[tokio::test]
    async fn reports_ok_when_redis_answers() {
        let url = test_redis::url();
        assert_eq!(check(&url).await, (StatusCode::OK, "ok\n".into()));
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
    async fn reports_unavailable_when_redis_refuses() {
        let url = format!("redis://127.0.0.1:{}", closed_port().await);
        assert_eq!(
            check(&url).await,
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Redis is unreachable\n".into()
            )
        );
    }

    #[tokio::test]
    async fn reports_unavailable_when_redis_never_answers() {
        // Bound and never accepted from: connections complete, and nothing
        // is ever said on them.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("redis://{}", listener.local_addr().unwrap());
        assert_eq!(
            check(&url).await,
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Redis is unreachable\n".into()
            )
        );
    }
}
