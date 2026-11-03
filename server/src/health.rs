//! The health check, which reports whether transom can reach Redis.

use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use redis::AsyncConnectionConfig;

/// How long the health check waits for Redis, both to connect and to answer.
/// A Redis that accepts the connection and never answers would otherwise
/// hold the check open until the prober gives up, and the prober would
/// learn nothing about why.
const REDIS_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) fn router(redis: redis::Client) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .with_state(redis)
}

async fn healthz(State(redis): State<redis::Client>) -> (StatusCode, &'static str) {
    match ping(&redis).await {
        Ok(()) => (StatusCode::OK, "ok\n"),
        Err(error) => {
            tracing::warn!(%error, "Redis is unreachable");
            (StatusCode::SERVICE_UNAVAILABLE, "Redis is unreachable\n")
        }
    }
}

async fn ping(redis: &redis::Client) -> redis::RedisResult<()> {
    let config = AsyncConnectionConfig::new()
        .set_connection_timeout(Some(REDIS_TIMEOUT))
        .set_response_timeout(Some(REDIS_TIMEOUT));
    let mut connection = redis
        .get_multiplexed_async_connection_with_config(&config)
        .await?;
    redis::cmd("PING").query_async(&mut connection).await
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tokio::net::TcpListener;
    use tower::ServiceExt;

    use super::router;

    async fn check(redis_url: &str) -> (StatusCode, String) {
        let redis = redis::Client::open(redis_url).unwrap();
        let request = Request::get("/healthz").body(Body::empty()).unwrap();
        let response = router(redis).oneshot(request).await.unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    fn redis_url() -> String {
        std::env::var("TRANSOM_TEST_REDIS_URL")
            .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string())
    }

    #[tokio::test]
    async fn reports_ok_when_redis_answers() {
        assert_eq!(check(&redis_url()).await, (StatusCode::OK, "ok\n".into()));
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

    /// A server that accepts connections and never answers them.
    async fn silent_server() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    #[tokio::test]
    async fn reports_unavailable_when_redis_never_answers() {
        let (_listener, port) = silent_server().await;
        let url = format!("redis://127.0.0.1:{port}");
        assert_eq!(
            check(&url).await,
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Redis is unreachable\n".into()
            )
        );
    }
}
