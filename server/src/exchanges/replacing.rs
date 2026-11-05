//! Replacing: a message with a `key` replaces the exchange with the same
//! key that its sender last sent the same recipient, while that one is
//! open. The earlier exchange ends as `replaced`, and the new one names it
//! in `replaces`, so a client shows one notification per key.
//!
//! `keyed:{sender}:{recipient}:{key}` holds the id of the latest exchange
//! with that key to open. A replaced exchange is kept until retention
//! deletes it, as any other that ended.

use std::sync::LazyLock;

use redis::Script;

use super::records::{self, Exchange, State};
use crate::app::{App, Failure};

/// Points the key's index at the new exchange and replaces the one it
/// pointed at, if that one is still open, in one step, so two messages
/// with the same key that open at once leave exactly one of them open.
static REPLACE: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        local index, exchange, expiries = KEYS[1], KEYS[2], KEYS[3]
        local id, prefix, index_name = ARGV[1], ARGV[2], ARGV[3]
        local earlier = redis.call('GET', index)
        redis.call('SET', index, id)
        redis.call('HSET', exchange, 'keyed', index_name)
        if not earlier or earlier == id then
            return false
        end
        local earlier_key = prefix .. 'exchange:' .. earlier
        if redis.call('HGET', earlier_key, 'state') ~= 'open' then
            return false
        end
        redis.call('HSET', earlier_key, 'state', 'replaced')
        redis.call('ZREM', expiries, earlier)
        redis.call('HSET', exchange, 'replaces', earlier)
        return earlier
        ",
    )
});

/// Replaces the open exchange with `exchange`'s key, once `exchange`
/// opens, and announces that the earlier one ended. A message held for
/// consent replaces nothing until it is let through.
pub(super) async fn replace(app: &App, exchange: &mut Exchange) -> Result<(), Failure> {
    let (Some(key), State::Open) = (&exchange.message.key, exchange.state) else {
        return Ok(());
    };
    let index = format!("keyed:{}:{}:{key}", exchange.sender, exchange.recipient);
    let mut connection = app.store.connection().await?;
    let replaced: Option<String> = REPLACE
        .key(app.store.key(&index))
        .key(app.store.key(&records::key(&exchange.id)))
        .key(app.store.key("expiries"))
        .arg(&exchange.id)
        .arg(app.store.key(""))
        .arg(&index)
        .invoke_async(&mut connection)
        .await?;
    exchange.replaces.clone_from(&replaced);
    let Some(earlier) = replaced else {
        return Ok(());
    };
    let earlier = super::visible(app, &earlier, &exchange.sender).await?;
    super::announce(app, &earlier).await
}
