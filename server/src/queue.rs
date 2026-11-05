//! The queues every server drains in its maintenance pass: webhook
//! deliveries, pushes, wake-ups, and the repeats of urgent exchanges.
//!
//! A queue is a sorted set of item ids by when each is next due, and each
//! item's fields, if it has any, are a hash of its own. A server claims a
//! due item by moving it a lease into the future, in one script, so no
//! other server takes it while this one tries it. When the attempt ends, the server
//! removes the item or schedules it again. A server that stops in the
//! middle of an attempt does neither, and the item comes due again when
//! its lease runs out, for any server to take: an item is attempted at
//! least once, and never by two servers at the same time, as long as each
//! attempt ends within its lease.

#[cfg(test)]
mod tests;

use std::sync::LazyLock;
use std::time::Duration;

use redis::{AsyncCommands, Script};

use crate::app::App;
use crate::store::Store;

pub(crate) struct Queue {
    /// The sorted set's name, under the store's prefix.
    name: &'static str,
    /// What each item's hash is named for: `{item}:{id}`.
    item: &'static str,
    /// How long a claim keeps other servers off an item: longer than any
    /// attempt takes, with its timeouts.
    lease: Duration,
}

/// Claims an item that is due, by moving it a lease into the future. An
/// item another server holds is due later, and one another server
/// finished is gone, so neither is claimed.
static CLAIM: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        local due = redis.call('ZSCORE', KEYS[1], ARGV[1])
        if not due or tonumber(due) > tonumber(ARGV[2]) then
            return 0
        end
        redis.call('ZADD', KEYS[1], ARGV[3], ARGV[1])
        return 1
        ",
    )
});

impl Queue {
    pub(crate) const fn new(name: &'static str, item: &'static str, lease: Duration) -> Self {
        Self { name, item, lease }
    }

    /// Adds `id`, with `fields`, to the queue, due at `due`, to `pipe`, so
    /// that it is queued in the same transaction as the change that
    /// queues it. An id already queued keeps the time it is due. An item
    /// whose id says all it needs has no `fields`, and no hash.
    pub(crate) fn add(
        &self,
        pipe: &mut redis::Pipeline,
        store: &Store,
        id: &str,
        fields: &[(&str, &str)],
        due: i64,
    ) {
        if !fields.is_empty() {
            pipe.hset_multiple(self.item(store, id), fields);
        }
        pipe.cmd("ZADD")
            .arg(store.key(self.name))
            .arg("NX")
            .arg(due)
            .arg(id);
    }

    /// The ids of the items due by `now`, which may already be claimed by
    /// the time this server tries to claim each.
    pub(crate) async fn due(&self, store: &Store, now: i64) -> redis::RedisResult<Vec<String>> {
        let mut connection = store.connection().await?;
        connection
            .zrangebyscore(store.key(self.name), "-inf", now)
            .await
    }

    /// Claims item `id` for this server, if it is still due at `now`, until
    /// its lease runs out. A server that is stopping claims nothing, so it
    /// starts no attempt it might not finish.
    pub(crate) async fn claim(&self, app: &App, id: &str, now: i64) -> redis::RedisResult<bool> {
        if app.stopping.is_cancelled() {
            return Ok(false);
        }
        let mut connection = app.store.connection().await?;
        let claimed: i64 = CLAIM
            .key(app.store.key(self.name))
            .arg(id)
            .arg(now)
            .arg(now.saturating_add(i64::try_from(self.lease.as_millis()).unwrap_or(i64::MAX)))
            .invoke_async(&mut connection)
            .await?;
        Ok(claimed == 1)
    }

    /// The fields of item `id`, which are empty for an item that is gone.
    pub(crate) async fn fields(
        &self,
        store: &Store,
        id: &str,
    ) -> redis::RedisResult<std::collections::HashMap<String, String>> {
        let mut connection = store.connection().await?;
        connection.hgetall(self.item(store, id)).await
    }

    /// Removes item `id`, which needs no more attempts.
    pub(crate) async fn finish(&self, store: &Store, id: &str) -> redis::RedisResult<()> {
        let mut connection = store.connection().await?;
        let mut pipe = redis::pipe();
        pipe.atomic();
        self.finishing(&mut pipe, store, id);
        pipe.query_async(&mut connection).await
    }

    /// Removes item `id` in `pipe`, so that an attempt's effects and the
    /// item's removal are written in one transaction.
    pub(crate) fn finishing(&self, pipe: &mut redis::Pipeline, store: &Store, id: &str) {
        pipe.zrem(store.key(self.name), id)
            .del(self.item(store, id));
    }

    /// Schedules item `id` again at `due`, counting `attempts` so far.
    pub(crate) async fn retry(
        &self,
        store: &Store,
        id: &str,
        attempts: i64,
        due: i64,
    ) -> redis::RedisResult<()> {
        let mut connection = store.connection().await?;
        let mut pipe = redis::pipe();
        pipe.atomic()
            .hset(self.item(store, id), "attempts", attempts)
            .zadd(store.key(self.name), id, due);
        pipe.query_async(&mut connection).await
    }

    fn item(&self, store: &Store, id: &str) -> String {
        store.key(&format!("{}:{id}", self.item))
    }
}
