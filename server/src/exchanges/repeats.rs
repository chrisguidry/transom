//! Repeats: an `urgent` exchange is shown again 5, 15, and 30 minutes
//! after it opens, to each recipient who has not read it, until it ends.
//! Android softens notifications posted closer than two minutes apart, so
//! the repeats are spaced wider than that.
//!
//! `repeats` is a queue (`queue.rs`) of `{id}:{n}`, the `n`th repeat of
//! exchange `id`, by when each is due, with no fields. Each repeat counts
//! in the exchange's `repeats` field, which its view shows, so a phone
//! that syncs can tell a repeat from a change it already showed.

use std::time::Duration;

use super::changes;
use super::message::Urgency;
use super::records::{self, Exchange, Found, State};
use crate::app::{App, Failure};
use crate::identities::records as identities;
use crate::queue::Queue;

/// The repeats waiting to be shown. Showing one reads the exchange and its
/// readers and queues its pushes, which a minute's lease leaves plenty of
/// room for.
const REPEATS: Queue = Queue::new("repeats", "repeat", Duration::from_secs(60));

const MINUTE: i64 = 60 * 1000;

/// How long after an urgent exchange opens each repeat is due.
const AFTER: [i64; 3] = [5 * MINUTE, 15 * MINUTE, 30 * MINUTE];

/// Schedules the repeats of `exchange` in `pipe`, if it is urgent and has
/// opened at `now`. Scheduling again leaves the times alone.
pub(super) fn schedule(pipe: &mut redis::Pipeline, app: &App, exchange: &Exchange, now: i64) {
    if exchange.state != State::Open || exchange.message.urgency != Urgency::Urgent {
        return;
    }
    for (n, after) in (1..).zip(AFTER) {
        let repeat = format!("{}:{n}", exchange.id);
        REPEATS.add(pipe, &app.store, &repeat, &[], now + after);
    }
}

/// Shows again each urgent exchange whose repeat is due by `now`: it
/// pushes the arrival again to each recipient who has not read it, wakes
/// their phones, and tells the live feeds, and counts the repeat, all in
/// one transaction with the repeat's removal. Returns how many exchanges
/// it repeated. Servers sharing a Redis can all run this: each claims a
/// repeat before it shows it, and a repeat of an exchange that is no
/// longer open is dropped.
pub(crate) async fn repeat_due(app: &App, now: i64) -> Result<usize, Failure> {
    let store = &app.store;
    let mut repeated = 0;
    for repeat in REPEATS.due(store, now).await? {
        if !REPEATS.claim(app, &repeat, now).await? {
            continue;
        }
        let (id, _) = repeat.rsplit_once(':').unwrap_or_default();
        let exchange = match records::load(store, id).await? {
            Found::Exchange(exchange) if exchange.state == State::Open => exchange,
            _ => {
                REPEATS.finish(store, &repeat).await?;
                continue;
            }
        };
        let mut pipe = redis::pipe();
        pipe.atomic();
        let mut to = vec![exchange.recipient.clone()];
        to.extend(identities::members(store, &exchange.recipient).await?);
        for identity in to.iter().filter(|identity| **identity != exchange.sender) {
            if !records::is_read(store, identity, id).await? {
                crate::push::repeat(app, identity, &exchange, now, &mut pipe).await?;
            }
        }
        pipe.hincr(store.key(&records::key(id)), "repeats", 1);
        // A phone that holds the live feed open hears the repeat as a
        // change to the exchange.
        changes::publish(&mut pipe, store, &exchange);
        REPEATS.finishing(&mut pipe, store, &repeat);
        let mut connection = store.connection().await?;
        pipe.query_async::<()>(&mut connection).await?;
        repeated += 1;
    }
    Ok(repeated)
}
