//! Retention: transom deletes an exchange once the `retention` setting has
//! passed since it ended, or for a notification, since it arrived. An open
//! question is never deleted for its age; its own time to live bounds it.
//!
//! Each exchange to delete waits in `deletions`, a sorted set of ids by
//! when each is due. The maintenance pass deletes what is due, the
//! live feeds say each one is gone, and browsers close a notification
//! of one that may still show.

use std::sync::LazyLock;
use std::time::Duration;

use futures_util::TryStreamExt;
use redis::{AsyncCommands, Script};
use tokio::sync::OnceCell;

use super::records::{self, Exchange, Found, State};
use crate::app::{App, Failure};

const DELETIONS: &str = "deletions";

/// How many exchanges one pass deletes at most, so a pass after a long
/// time without one, or the first over data from before retention, stays
/// short and the passes after it delete the rest.
const PER_PASS: isize = 500;

/// How long exchanges are kept, and whether this server has scheduled the
/// exchanges that ended before retention existed.
pub(crate) struct Retention {
    kept: Duration,
    backfilled: OnceCell<()>,
}

impl Retention {
    pub(crate) fn new(kept: Duration) -> Self {
        Self {
            kept,
            backfilled: OnceCell::new(),
        }
    }

    pub(crate) fn milliseconds(&self) -> i64 {
        i64::try_from(self.kept.as_millis()).unwrap_or(i64::MAX)
    }
}

/// Whether `exchange` is aging now: a question once it ends, and a
/// notification once it arrives, which for one held for consent is when it
/// is let through.
fn aging(exchange: &Exchange) -> bool {
    exchange.state.is_final() || (exchange.state == State::Open && !exchange.message.is_question())
}

/// Schedules `exchange`'s deletion when it starts to age, at `now`. A
/// notification is scheduled when it arrives, and its later end leaves
/// that time alone. The deletion is scheduled in `pipe`.
pub(super) fn schedule(pipe: &mut redis::Pipeline, app: &App, exchange: &Exchange, now: i64) {
    if !aging(exchange) {
        return;
    }
    let due = now.saturating_add(app.retention.milliseconds());
    pipe.cmd("ZADD")
        .arg(app.store.key(DELETIONS))
        .arg("NX")
        .arg(due)
        .arg(&exchange.id);
}

/// When an exchange from before retention existed started to age, from
/// what it records: a notification's creation, a reply's time, or an
/// expiry. A withdrawal, a decline, or a replacement records no time, so
/// one of those ages from `now`, and is kept a whole retention from the
/// backfill rather than less.
fn aged_from(exchange: &Exchange, now: i64) -> Option<i64> {
    if exchange.state == State::Pending {
        return None;
    }
    if !exchange.message.is_question() && exchange.state != State::Declined {
        return Some(exchange.created);
    }
    match exchange.state {
        State::Replied => Some(exchange.reply.as_ref().map_or(now, |reply| reply.at)),
        State::Expired => Some(exchange.expires.unwrap_or(now)),
        State::Withdrawn | State::Declined | State::Replaced => Some(now),
        State::Open | State::Pending => None,
    }
}

/// Schedules every exchange that ended, or notification that arrived,
/// without a deletion scheduled, by when it ended or arrived, and returns
/// how many it looked at. Scheduling leaves a scheduled exchange alone, so
/// this can run again at any time and change nothing.
pub(crate) async fn backfill(app: &App, now: i64) -> Result<usize, Failure> {
    let mut connection = app.store.connection().await?;
    let prefix = app.store.key(&records::key(""));
    let keys: Vec<String> = connection
        .scan_match(format!("{prefix}*"))
        .await?
        .try_collect()
        .await?;
    let mut scheduling = redis::cmd("ZADD");
    scheduling.arg(app.store.key(DELETIONS)).arg("NX");
    let mut any = false;
    for key in &keys {
        let id = key.strip_prefix(&prefix).unwrap_or(key);
        let Found::Exchange(exchange) = records::load(&app.store, id).await? else {
            continue;
        };
        if let Some(from) = aged_from(&exchange, now) {
            scheduling
                .arg(from.saturating_add(app.retention.milliseconds()))
                .arg(id);
            any = true;
        }
    }
    if any {
        let _: i64 = scheduling.query_async(&mut connection).await?;
    }
    Ok(keys.len())
}

/// Runs the backfill once in this server's life, at its first pass that
/// reaches Redis.
pub(crate) async fn backfill_once(app: &App) -> Result<(), Failure> {
    app.retention
        .backfilled
        .get_or_try_init(|| async {
            let looked = backfill(app, records::now()).await?;
            tracing::info!(exchanges = looked, "scheduled what retention deletes");
            Ok::<(), Failure>(())
        })
        .await?;
    Ok(())
}

/// Deletes one exchange whose time has come, if this server is the first
/// to take it from `deletions`, and says on its channels that it is gone.
/// It takes the exchange out of its sender's and recipient's lists, the
/// expiry index, its key's index while that still names it, a hold for
/// consent, and the read state of its recipient, the recipient's members,
/// and every reader the exchange records, which includes members who have
/// since left. A key whose exchange has no
/// sender, such as a deleted group's tombstone, is deleted alone.
static DELETE: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        local id, prefix = ARGV[1], ARGV[2]
        if redis.call('ZREM', KEYS[1], id) == 0 then
            return 0
        end
        local fields = redis.call('HGETALL', KEYS[2])
        redis.call('DEL', KEYS[2], KEYS[3])
        local exchange, readers = {}, {}
        for i = 1, #fields, 2 do
            local reader = string.match(fields[i], '^reader:(.*)$')
            if reader then
                readers[#readers + 1] = reader
            else
                exchange[fields[i]] = fields[i + 1]
            end
        end
        local sender, recipient = exchange['sender'], exchange['recipient']
        if not sender or not recipient then
            return 0
        end
        redis.call('ZREM', prefix .. 'sent:' .. sender, id)
        redis.call('ZREM', prefix .. 'received:' .. recipient, id)
        redis.call('ZREM', prefix .. 'expiries', id)
        local keyed = exchange['keyed']
        if keyed and redis.call('GET', prefix .. keyed) == id then
            redis.call('DEL', prefix .. keyed)
        end
        redis.call('LREM', prefix .. 'held:' .. recipient .. ':' .. sender, 0, id)
        readers[#readers + 1] = recipient
        for _, member in ipairs(redis.call('SMEMBERS', prefix .. 'members:' .. recipient)) do
            readers[#readers + 1] = member
        end
        for _, reader in ipairs(readers) do
            redis.call('SREM', prefix .. 'read:' .. reader, id)
        end
        redis.call('PUBLISH', prefix .. 'events:exchange:' .. id, 'gone')
        redis.call('PUBLISH', prefix .. 'events:identity:' .. sender, id)
        redis.call('PUBLISH', prefix .. 'events:identity:' .. recipient, id)
        return 1
        ",
    )
});

/// Deletes the exchanges due by `now`, at most [`PER_PASS`], and returns
/// how many it deleted. Two servers sharing a Redis can both run this:
/// each exchange is taken from `deletions` and deleted in one script, so
/// only one of them deletes it and announces it.
pub(crate) async fn delete_due(app: &App, now: i64) -> Result<usize, Failure> {
    let mut connection = app.store.connection().await?;
    let due: Vec<String> = connection
        .zrangebyscore_limit(app.store.key(DELETIONS), "-inf", now, 0, PER_PASS)
        .await?;
    let mut deleted = 0;
    for id in due {
        // Who may still show a notification is read before the deletion,
        // which takes the read state with it.
        let closing = match records::load(&app.store, &id).await? {
            Found::Exchange(exchange) => {
                let showing = crate::push::showing(app, &exchange).await?;
                Some((exchange, showing))
            }
            Found::Gone | Found::Missing => None,
        };
        let done: i64 = DELETE
            .key(app.store.key(DELETIONS))
            .key(app.store.key(&records::key(&id)))
            .key(app.store.key(&super::attachments::key(&id)))
            .arg(&id)
            .arg(app.store.key(""))
            .invoke_async(&mut connection)
            .await?;
        if done == 1
            && let Some((exchange, showing)) = &closing
        {
            crate::push::gone(app, exchange, showing).await?;
        }
        deleted += usize::try_from(done).unwrap_or_default();
    }
    Ok(deleted)
}
