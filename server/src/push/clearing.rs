//! Clearing notifications of many exchanges at once, and of an exchange
//! that retention deletes.

use serde_json::json;

use super::delivery::{self, Push};
use super::{KEEP_ENDED, devices, wakes};
use crate::app::{App, Failure};
use crate::exchanges::message::Urgency;
use crate::exchanges::records::{self, Exchange, State};
use crate::identities::records as identities;

/// How many exchange ids one push carries. A push service takes at most
/// 4096 bytes, and 100 ids of 26 characters each leave room for the rest
/// of the payload and the encryption's own bytes.
const IDS_PER_PUSH: usize = 100;

/// Tells `reader`'s own devices that it read each of `exchanges`, so each
/// clears those notifications: each browser in one push of the ids, or a
/// few when there are more than one push holds, and each phone in one
/// wake-up, after which it syncs and finds them read. Each push's urgency
/// is that of its most urgent exchange, so it reaches a browser as soon
/// as the notifications it clears did. A browser shows no notification
/// of a very-low exchange, so it hears of none.
pub(crate) async fn read_many(
    app: &App,
    reader: &str,
    exchanges: &[&Exchange],
) -> Result<(), Failure> {
    if exchanges.is_empty() {
        return Ok(());
    }
    let now = records::now();
    let devices = devices::of(app, reader).await?;
    let shown: Vec<&Exchange> = exchanges
        .iter()
        .copied()
        .filter(|exchange| exchange.message.urgency != Urgency::VeryLow)
        .collect();
    for batch in shown.chunks(IDS_PER_PUSH) {
        let ids: Vec<&str> = batch.iter().map(|exchange| exchange.id.as_str()).collect();
        let payload = json!({ "event": "read", "ids": ids }).to_string();
        let most_urgent = batch
            .iter()
            .max_by_key(|exchange| exchange.message.urgency)
            .expect("a chunk is never empty");
        // A topic of its own: a push for many exchanges must replace
        // neither an arrival nor another batch that a push service holds.
        let topic = ulid::Ulid::generate().to_string();
        let push = Push {
            payload: &payload,
            urgency: most_urgent.message.urgency.web_push(),
            ttl: KEEP_ENDED,
            topic: &topic,
        };
        delivery::enqueue(app, reader, &devices, &push, now).await?;
    }
    wakes::enqueue(app, reader, &devices, now).await
}

/// The identities that may still show a notification of `exchange`: while
/// it is open, each recipient but its sender who has not read it. An end
/// or a read has already cleared the notification everywhere else, and a
/// browser never showed one of a very-low exchange.
pub(crate) async fn showing(app: &App, exchange: &Exchange) -> Result<Vec<String>, Failure> {
    if exchange.state != State::Open || exchange.message.urgency == Urgency::VeryLow {
        return Ok(Vec::new());
    }
    let mut to = vec![exchange.recipient.clone()];
    to.extend(identities::members(&app.store, &exchange.recipient).await?);
    let mut showing = Vec::new();
    for identity in to {
        if identity != exchange.sender
            && !records::is_read(&app.store, &identity, &exchange.id).await?
        {
            showing.push(identity);
        }
    }
    Ok(showing)
}

/// Closes the browser notifications of `exchange`, which retention
/// deleted, for each of `showing`. Its `Topic` replaces an arrival that a
/// push service still holds for a browser that is offline.
///
/// Phones are not woken. The gateway sends each wake-up at high priority,
/// and FCM may send an app's wake-ups at normal priority once too many of
/// them show nothing, which would delay the arrivals that matter. A phone
/// drops the notification of an exchange that has left the list at its
/// next sync anyway.
pub(crate) async fn gone(
    app: &App,
    exchange: &Exchange,
    showing: &[String],
) -> Result<(), Failure> {
    let payload = json!({ "event": "gone", "id": exchange.id }).to_string();
    let push = Push {
        payload: &payload,
        urgency: exchange.message.urgency.web_push(),
        ttl: KEEP_ENDED,
        topic: &exchange.id,
    };
    let now = records::now();
    for identity in showing {
        let devices = devices::of(app, identity).await?;
        delivery::enqueue(app, identity, &devices, &push, now).await?;
    }
    Ok(())
}
