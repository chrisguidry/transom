//! Replacing: a message with a `key` replaces the exchange with the same
//! key that its sender last sent the same recipient, while that one is
//! open. The earlier exchange ends as `replaced`, and the new one names it
//! in `replaces`, so a client shows one notification per key.
//!
//! `keyed:{sender}:{recipient}:{key}` holds the id of the latest exchange
//! with that key to open. A replaced exchange is kept until retention
//! deletes it, as any other that ended.

use std::sync::LazyLock;

use redis::{AsyncCommands, Script};

use super::records::{self, Exchange, Found, State};
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

/// A replacement written in a transaction, and what it expected to
/// replace.
pub(super) struct Replacing {
    /// Where the script's answer is among the transaction's answers.
    answer: usize,
    expected: Option<String>,
}

/// The index of `exchange`'s key, once it opens with one.
fn index(exchange: &Exchange) -> Option<String> {
    let (Some(key), State::Open) = (&exchange.message.key, exchange.state) else {
        return None;
    };
    Some(format!(
        "keyed:{}:{}:{key}",
        exchange.sender, exchange.recipient
    ))
}

/// Replaces the open exchange with `exchange`'s key, once `exchange`
/// opens, in `pipe`, which also creates or opens `exchange`. A message held
/// for consent replaces nothing until it is let through.
///
/// What it replaces is read first, so that `exchange`'s own announcement
/// names it and the earlier exchange's end is announced in the same
/// transaction. The script decides in that transaction; when another
/// message with the key changed the index in between, [`Replacing::settle`]
/// announces what the script did replace.
pub(super) async fn replace(
    app: &App,
    exchange: &mut Exchange,
    pipe: &mut redis::Pipeline,
) -> Result<Option<Replacing>, Failure> {
    let Some(index) = index(exchange) else {
        return Ok(None);
    };
    let mut connection = app.store.connection().await?;
    let named: Option<String> = connection.get(app.store.key(&index)).await?;
    let earlier = match named {
        Some(id) if id != exchange.id => match records::load(&app.store, &id).await? {
            Found::Exchange(earlier) if earlier.state == State::Open => Some(*earlier),
            _ => None,
        },
        _ => None,
    };
    exchange.replaces = earlier.as_ref().map(|earlier| earlier.id.clone());
    if let Some(mut earlier) = earlier {
        earlier.state = State::Replaced;
        super::announcing(app, &earlier, pipe).await?;
    }
    // A transaction does not load a script it lacks, so it loads it
    // first.
    pipe.load_script(&REPLACE);
    let answer = pipe.len();
    let mut invocation = REPLACE.prepare_invoke();
    invocation
        .key(app.store.key(&index))
        .key(app.store.key(&records::key(&exchange.id)))
        .key(app.store.key("expiries"))
        .arg(&exchange.id)
        .arg(app.store.key(""))
        .arg(&index);
    pipe.invoke_script(&invocation);
    Ok(Some(Replacing {
        answer,
        expected: exchange.replaces.clone(),
    }))
}

impl Replacing {
    /// Announces the end of what the script replaced, when that is not
    /// what [`replace`] expected, from the transaction's `answers`, and
    /// names it in `exchange`.
    pub(super) async fn settle(
        self,
        app: &App,
        exchange: &mut Exchange,
        answers: &[redis::Value],
    ) -> Result<(), Failure> {
        let replaced: Option<String> = answers
            .get(self.answer)
            .and_then(|answer| redis::from_redis_value_ref(answer).ok())
            .flatten();
        if replaced == self.expected {
            return Ok(());
        }
        exchange.replaces.clone_from(&replaced);
        let Some(earlier) = replaced else {
            return Ok(());
        };
        let earlier = super::visible(app, &earlier, &exchange.sender).await?;
        super::announce(app, &earlier).await
    }
}

/// Writes `pipe`, a transaction, and settles the replacement in it, if it
/// holds one.
pub(super) async fn write(
    app: &App,
    exchange: &mut Exchange,
    pipe: &redis::Pipeline,
    replacing: Option<Replacing>,
) -> Result<(), Failure> {
    let mut connection = app.store.connection().await?;
    let answers: Vec<redis::Value> = pipe.query_async(&mut connection).await?;
    if let Some(replacing) = replacing {
        replacing.settle(app, exchange, &answers).await?;
    }
    Ok(())
}
