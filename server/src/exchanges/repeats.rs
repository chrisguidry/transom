//! Repeats: an `urgent` exchange is shown again 5, 15, and 30 minutes
//! after it opens, to each recipient who has not read it, until it ends.
//! Android softens notifications posted closer than two minutes apart, so
//! the repeats are spaced wider than that.
//!
//! `repeats` is a sorted set of `{id}:{n}`, the `n`th repeat of exchange
//! `id`, by when each is due. Each repeat counts in the exchange's
//! `repeats` field, which its view shows, so a phone that syncs can tell a
//! repeat from a change it already showed.

use std::sync::LazyLock;

use redis::{AsyncCommands, Script};

use super::changes;
use super::message::Urgency;
use super::records::{self, Exchange, Found, State};
use crate::app::{App, Failure};
use crate::identities::records as identities;

const REPEATS: &str = "repeats";

const MINUTE: i64 = 60 * 1000;

/// How long after an urgent exchange opens each repeat is due.
const AFTER: [i64; 3] = [5 * MINUTE, 15 * MINUTE, 30 * MINUTE];

/// Schedules the repeats of `exchange`, if it is urgent and has opened at
/// `now`. Scheduling again leaves the times alone.
pub(super) async fn schedule(app: &App, exchange: &Exchange, now: i64) -> Result<(), Failure> {
    if exchange.state != State::Open || exchange.message.urgency != Urgency::Urgent {
        return Ok(());
    }
    let mut scheduling = redis::cmd("ZADD");
    scheduling.arg(app.store.key(REPEATS)).arg("NX");
    for (n, after) in (1..).zip(AFTER) {
        scheduling
            .arg(now + after)
            .arg(format!("{}:{n}", exchange.id));
    }
    let mut connection = app.store.connection().await?;
    let _: i64 = scheduling.query_async(&mut connection).await?;
    Ok(())
}

/// Takes one repeat from `repeats` and counts it, if this server is the
/// first to take it and the exchange is still open.
static CLAIM: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        if redis.call('ZREM', KEYS[1], ARGV[1]) == 0 then
            return 0
        end
        if redis.call('HGET', KEYS[2], 'state') ~= 'open' then
            return 0
        end
        redis.call('HINCRBY', KEYS[2], 'repeats', 1)
        return 1
        ",
    )
});

/// Shows again each urgent exchange whose repeat is due by `now`: it
/// pushes the arrival again to each recipient who has not read it, wakes
/// their phones, and tells the live feeds. Returns how many exchanges it
/// repeated. Servers sharing a Redis can all run this: claiming a repeat
/// takes it from `repeats`, so only one server sends it.
pub(crate) async fn repeat_due(app: &App, now: i64) -> Result<usize, Failure> {
    let mut connection = app.store.connection().await?;
    let due: Vec<String> = connection
        .zrangebyscore(app.store.key(REPEATS), "-inf", now)
        .await?;
    let mut repeated = 0;
    for repeat in due {
        let (id, _) = repeat.rsplit_once(':').unwrap_or_default();
        let claimed: i64 = CLAIM
            .key(app.store.key(REPEATS))
            .key(app.store.key(&records::key(id)))
            .arg(&repeat)
            .invoke_async(&mut connection)
            .await?;
        let (1, Found::Exchange(exchange)) = (claimed, records::load(&app.store, id).await?) else {
            continue;
        };
        let mut to = vec![exchange.recipient.clone()];
        to.extend(identities::members(&app.store, &exchange.recipient).await?);
        for identity in to.iter().filter(|identity| **identity != exchange.sender) {
            if records::read_ids(&app.store, identity).await?.contains(id) {
                continue;
            }
            crate::push::repeat(app, identity, &exchange, now).await?;
        }
        // A phone that holds the live feed open hears the repeat as a
        // change to the exchange.
        changes::publish(&app.store, &exchange).await?;
        repeated += 1;
    }
    Ok(repeated)
}
