//! Keeping this computer's copy of the inbox what the server says it is.
//! The server is the system of record: the app follows the live feed,
//! reads the whole list each time the feed opens, and queues what the
//! person does here in the outbox until the server has it.

pub mod client;
pub mod feed;
pub mod outbox;
pub mod wake;

use std::sync::Arc;

use tokio::sync::Notify;
use tokio::sync::mpsc::UnboundedSender;

use crate::exchange::Exchange;
use client::Client;
use feed::{Event, Feed, backoff};

/// What following the feed learns.
#[derive(Debug)]
pub enum Heard {
    /// The feed is connected, and this is the whole list as of then.
    Listed(Vec<Exchange>),
    /// One exchange changed.
    Changed(Box<Exchange>),
    /// The server no longer keeps this exchange.
    Gone(String),
    /// The connection was lost. Another is tried after a wait.
    Disconnected,
    /// The server no longer takes the session's token.
    SignedOut,
}

/// This computer's name, which the devices list shows.
pub fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|name| name.trim().to_owned())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Linux desktop".to_owned())
}

/// Follows `me`'s live feed until the session ends, reconnecting after a
/// backoff each time the connection fails or closes, or at once when
/// `woke` is notified. Each connection registers this computer as
/// `device` and reads the list once the feed is open: the feed sends only
/// what changes after that, and its events wait until the list is sent
/// on, so an event never loses to a list that was read before it.
pub async fn follow(
    client: Client,
    me: String,
    device: String,
    heard: UnboundedSender<Heard>,
    woke: Arc<Notify>,
) {
    let mut failures = 0;
    loop {
        let ended = match connect(&client, &me, &device, &heard, &woke).await {
            Ok(ended) => {
                failures = 0;
                ended
            }
            Err(failure) if failure.signed_out() => {
                let _ = heard.send(Heard::SignedOut);
                return;
            }
            Err(failure) => {
                tracing::info!(%failure, "the feed is not connected");
                failures += 1;
                Ended::Closed
            }
        };
        if heard.send(Heard::Disconnected).is_err() {
            return;
        }
        if ended == Ended::Woke {
            continue;
        }
        tokio::select! {
            () = tokio::time::sleep(backoff(failures)) => {}
            () = woke.notified() => failures = 0,
        }
    }
}

/// How a connection that had opened and listed came to its end.
#[derive(PartialEq, Eq)]
enum Ended {
    /// The server closed it, or it was lost.
    Closed,
    /// The computer woke or its network came back, so the next
    /// connection is made with no wait.
    Woke,
}

/// One connection, from its opening to its end. It returns `Ok` when a
/// connection that had opened and listed then ended, so the next try
/// follows the shortest wait.
async fn connect(
    client: &Client,
    me: &str,
    device: &str,
    heard: &UnboundedSender<Heard>,
    woke: &Notify,
) -> Result<Ended, client::Failure> {
    let mut feed = Feed::open(client, me).await?;
    let listed = client.exchanges(me).await?;
    if heard.send(Heard::Listed(listed)).is_err() {
        return Ok(Ended::Closed);
    }
    if let Err(failure) = client.register(me, device, &hostname()).await {
        tracing::warn!(%failure, "this computer could not be registered as a device");
    }
    loop {
        let event = tokio::select! {
            event = feed.next() => event,
            // The computer woke or changed networks, and the connection
            // it had may be dead without knowing it yet.
            () = woke.notified() => {
                tracing::info!("reconnecting the feed after a wake or a change of network");
                return Ok(Ended::Woke);
            }
        };
        let sent = match event {
            Ok(Some(Event::Changed(exchange))) => heard.send(Heard::Changed(exchange)),
            Ok(Some(Event::Gone(id))) => heard.send(Heard::Gone(id)),
            Ok(None) => {
                tracing::info!("the server closed the feed");
                return Ok(Ended::Closed);
            }
            Err(failure) => {
                tracing::info!(%failure, "the feed's connection ended");
                return Ok(Ended::Closed);
            }
        };
        if sent.is_err() {
            return Ok(Ended::Closed);
        }
    }
}

#[cfg(test)]
mod tests;
