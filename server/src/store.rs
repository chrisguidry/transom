//! transom's connection to Redis, which holds all of its data.
//!
//! transom connects to one Redis URL. A Redis run for failover keeps that
//! URL on its current primary, and while it moves, the URL refuses
//! connections for a few seconds and the connections already open drop.
//! The shared connection reconnects and sends a dropped command again, and
//! each [`Subscription`] subscribes again, so a failover delays transom
//! rather than failing it.

#[cfg(test)]
pub(crate) mod failover;
mod subscription;
#[cfg(test)]
mod tests;

use std::time::Duration;

use redis::aio::{ConnectionLike, ConnectionManager, ConnectionManagerConfig};
use redis::{AsyncConnectionConfig, Cmd, Pipeline, RedisFuture, Value};
use tokio::sync::OnceCell;

pub(crate) use subscription::{Heard, Subscription};

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

/// The shared connection. It reconnects by itself after a failure, and a
/// command whose connection had dropped goes out once more on the new
/// one, so the first request after a failover succeeds instead of failing
/// on the connection the old primary closed.
///
/// A command that reached Redis just before its connection dropped runs
/// twice. transom's writes set values and add to sets, and its scripts
/// check state before they change it, so a second run changes nothing
/// more. The one list it appends to, of messages held for consent,
/// releases each message once however often the list names it. A command
/// that timed out is not sent again, since it may still be running.
#[derive(Clone)]
pub(crate) struct Connection(ConnectionManager);

impl ConnectionLike for Connection {
    fn req_packed_command<'a>(&'a mut self, cmd: &'a Cmd) -> RedisFuture<'a, Value> {
        Box::pin(async move {
            match self.0.req_packed_command(cmd).await {
                Err(error) if error.is_connection_dropped() => self.0.req_packed_command(cmd).await,
                answered => answered,
            }
        })
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        pipeline: &'a Pipeline,
        offset: usize,
        count: usize,
    ) -> RedisFuture<'a, Vec<Value>> {
        Box::pin(async move {
            match self.0.req_packed_commands(pipeline, offset, count).await {
                Err(error) if error.is_connection_dropped() => {
                    self.0.req_packed_commands(pipeline, offset, count).await
                }
                answered => answered,
            }
        })
    }

    fn get_db(&self) -> i64 {
        self.0.get_db()
    }
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

    /// The shared connection.
    pub(crate) async fn connection(&self) -> redis::RedisResult<Connection> {
        self.connection
            .get_or_try_init(|| {
                // Eight retries, the longest a second apart, wait about five
                // seconds for Redis to take connections again: long enough
                // to ride out a failover, and short enough that a request
                // made while Redis is down fails while its client still
                // waits for the answer.
                let config = ConnectionManagerConfig::new()
                    .set_connection_timeout(Some(REDIS_TIMEOUT))
                    .set_response_timeout(Some(REDIS_TIMEOUT))
                    .set_number_of_retries(8)
                    .set_max_delay(Duration::from_secs(1));
                ConnectionManager::new_with_config(self.client.clone(), config)
            })
            .await
            .cloned()
            .map(Connection)
    }

    /// A connection of its own that subscribes to `channels`, each named
    /// under the store's prefix. Waiting requests and event streams each
    /// hold one for as long as they wait.
    pub(crate) async fn subscribe(&self, channels: &[String]) -> redis::RedisResult<Subscription> {
        let channels = channels.iter().map(|channel| self.key(channel)).collect();
        Subscription::open(self.client.clone(), channels).await
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

    /// A store on the test Redis with a prefix no other test shares.
    pub(crate) fn store() -> Store {
        store_at(&test_redis::url())
    }

    pub(crate) fn store_at(url: &str) -> Store {
        prefixed(url, &format!("test-{}", ulid::Ulid::generate()))
    }

    /// A store at `url` under `prefix`, as a second server on the same
    /// Redis makes.
    pub(crate) fn prefixed(url: &str, prefix: &str) -> Store {
        Store::new(redis::Client::open(url).unwrap(), prefix.to_owned())
    }
}
