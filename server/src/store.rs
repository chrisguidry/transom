//! transom's connection to Redis, which holds all of its data.

use std::time::Duration;

use redis::AsyncConnectionConfig;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use tokio::sync::OnceCell;

/// How long transom waits for Redis, both to connect and to answer. A Redis
/// that accepts connections and never answers would otherwise hold every
/// request open until its client gave up.
const REDIS_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) struct Store {
    client: redis::Client,
    prefix: String,
    /// Made on first use, so transom starts, and reports itself unhealthy,
    /// even when Redis is down at startup.
    connection: OnceCell<ConnectionManager>,
}

impl Store {
    pub(crate) fn new(client: redis::Client, prefix: String) -> Self {
        Self {
            client,
            prefix,
            connection: OnceCell::new(),
        }
    }

    /// The full key for `name`, under this store's prefix.
    pub(crate) fn key(&self, name: &str) -> String {
        format!("{}:{name}", self.prefix)
    }

    /// The shared connection, which reconnects by itself after a failure.
    pub(crate) async fn connection(&self) -> redis::RedisResult<ConnectionManager> {
        self.connection
            .get_or_try_init(|| {
                // Two quick retries ride out a blip, such as Redis restarting.
                // The default of six, with exponential backoff, holds a
                // request for about nine seconds before it fails.
                let config = ConnectionManagerConfig::new()
                    .set_connection_timeout(Some(REDIS_TIMEOUT))
                    .set_response_timeout(Some(REDIS_TIMEOUT))
                    .set_number_of_retries(2)
                    .set_max_delay(Duration::from_millis(500));
                ConnectionManager::new_with_config(self.client.clone(), config)
            })
            .await
            .cloned()
    }

    /// A connection of its own that subscribes to `channels`, each named
    /// under the store's prefix. Waiting requests and event streams each
    /// hold one for as long as they wait.
    pub(crate) async fn subscribe(
        &self,
        channels: &[String],
    ) -> redis::RedisResult<redis::aio::PubSub> {
        let mut pubsub = self.client.get_async_pubsub().await?;
        for channel in channels {
            pubsub.subscribe(self.key(channel)).await?;
        }
        Ok(pubsub)
    }

    /// Checks that Redis answers, on a connection of its own, so a health
    /// check reports what a new request would find.
    pub(crate) async fn ping(&self) -> redis::RedisResult<()> {
        let config = AsyncConnectionConfig::new()
            .set_connection_timeout(Some(REDIS_TIMEOUT))
            .set_response_timeout(Some(REDIS_TIMEOUT));
        let mut connection = self
            .client
            .get_multiplexed_async_connection_with_config(&config)
            .await?;
        redis::cmd("PING").query_async(&mut connection).await
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::Store;

    pub(crate) fn redis_url() -> String {
        // unwrap_or rather than a closure, so the same code runs whether or not
        // the variable is set, and coverage is the same here and in CI.
        std::env::var("TRANSOM_TEST_REDIS_URL").unwrap_or("redis://127.0.0.1:6379".to_owned())
    }

    /// A store on the test Redis with a prefix no other test shares.
    pub(crate) fn store() -> Store {
        store_at(&redis_url())
    }

    pub(crate) fn store_at(url: &str) -> Store {
        Store::new(
            redis::Client::open(url).unwrap(),
            format!("test-{}", ulid::Ulid::generate()),
        )
    }
}
