//! Replies and read marks this person made here, kept in a file until
//! the server has them, so an answer clicked while the network is away is
//! never lost.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::client::{Client, Failure, Replied};
use crate::exchange::{Exchange, Reply};
use crate::inbox::Sending;

/// Where a reply was made, which decides how a lost one is told: a
/// window shows it, and anything else needs a notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Window,
    Notification,
    Tray,
}

/// One thing waiting to be sent.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Queued {
    Reply {
        exchange: String,
        #[serde(flatten)]
        sending: Sending,
        origin: Origin,
    },
    Read {
        exchange: String,
        read: bool,
    },
}

impl Queued {
    pub fn exchange(&self) -> &str {
        match self {
            Self::Reply { exchange, .. } | Self::Read { exchange, .. } => exchange,
        }
    }
}

/// What the server made of one queued thing.
#[derive(Debug)]
pub enum Settled {
    /// The reply is the exchange's reply.
    Accepted(Box<Exchange>),
    /// Someone else replied first, with `winner`, so the reply `sending`,
    /// made at `origin`, is not the exchange's.
    Lost {
        exchange: String,
        sending: Sending,
        origin: Origin,
        winner: Box<Reply>,
    },
    /// The server has `exchange` as read or unread now.
    Marked { exchange: String, read: bool },
    /// The server refused it in a way it would refuse again, such as a
    /// reply to an exchange that expired.
    Refused(String),
}

/// The queue, and the file that holds it. Every change is written before
/// it returns.
#[derive(Debug)]
pub struct Outbox {
    path: PathBuf,
    queued: Vec<Queued>,
}

impl Outbox {
    /// The outbox kept at `path`, with whatever it held when the app last
    /// stopped. A file that cannot be read is an empty outbox.
    pub fn open(path: &Path) -> Self {
        let queued = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path: path.to_owned(),
            queued,
        }
    }

    pub fn queued(&self) -> &[Queued] {
        &self.queued
    }

    /// The reply queued for `exchange`, if there is one.
    pub fn reply_to(&self, exchange: &str) -> Option<&Sending> {
        self.queued.iter().find_map(|queued| match queued {
            Queued::Reply {
                exchange: id,
                sending,
                ..
            } if id == exchange => Some(sending),
            _ => None,
        })
    }

    /// The read state queued for `exchange`, if there is one: what this
    /// person last decided, which wins over what the server last said.
    pub fn marked(&self, exchange: &str) -> Option<bool> {
        self.queued.iter().rev().find_map(|queued| match queued {
            Queued::Read { exchange: id, read } if id == exchange => Some(*read),
            _ => None,
        })
    }

    /// Queues `item`. A read mark replaces an earlier one for the same
    /// exchange, and a second reply to one exchange is dropped, since the
    /// first answer wins.
    pub fn put(&mut self, item: Queued) {
        match &item {
            Queued::Read { exchange, .. } => self.queued.retain(
                |queued| !matches!(queued, Queued::Read { exchange: id, .. } if id == exchange),
            ),
            Queued::Reply { exchange, .. } => {
                if self.reply_to(exchange).is_some() {
                    return;
                }
            }
        }
        self.queued.push(item);
        self.save();
    }

    /// Takes `item` off the queue, once the server has settled it.
    pub fn sent(&mut self, item: &Queued) {
        self.queued.retain(|queued| queued != item);
        self.save();
    }

    /// Empties the queue, as signing out does.
    pub fn clear(&mut self) {
        self.queued.clear();
        self.save();
    }

    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let bytes = serde_json::to_vec_pretty(&self.queued).expect("the queue is plain data");
        // Written beside the file and moved over it, so a crash part way
        // leaves the old queue instead of half of the new one.
        let partial = self.path.with_extension("partial");
        if let Err(error) =
            std::fs::write(&partial, bytes).and_then(|()| std::fs::rename(&partial, &self.path))
        {
            tracing::error!(%error, "the outbox could not be saved");
        }
    }
}

/// Sends `items` as `me`, in order, and reports each the server settled.
/// It stops at the first one that has to be sent again later, because the
/// server could not be reached or failed for now, and returns that
/// failure. Every exchange marked read goes in one request.
pub async fn deliver(
    client: &Client,
    me: &str,
    items: Vec<Queued>,
    mut settled: impl FnMut(Queued, Settled),
) -> Result<(), Failure> {
    let (reads, rest): (Vec<Queued>, Vec<Queued>) = items
        .into_iter()
        .partition(|item| matches!(item, Queued::Read { read: true, .. }));
    for item in rest {
        let outcome = match &item {
            Queued::Reply {
                exchange,
                sending,
                origin,
            } => client
                .reply(exchange, sending.choice.as_deref(), sending.text.as_deref())
                .await
                .map(|replied| match replied {
                    Replied::Accepted(exchange) => Settled::Accepted(exchange),
                    Replied::Lost(winner) => Settled::Lost {
                        exchange: exchange.clone(),
                        sending: sending.clone(),
                        origin: *origin,
                        winner,
                    },
                }),
            Queued::Read { exchange, read } => {
                let marked = Settled::Marked {
                    exchange: exchange.clone(),
                    read: *read,
                };
                client.mark(exchange, me, *read).await.map(|()| marked)
            }
        };
        match outcome {
            Ok(outcome) => settled(item, outcome),
            Err(failure) if failure.transient() => return Err(failure),
            Err(failure) => settled(item, Settled::Refused(failure.to_string())),
        }
    }
    if !reads.is_empty() {
        let ids: Vec<String> = reads
            .iter()
            .map(|item| item.exchange().to_owned())
            .collect();
        match client.mark_read(me, &ids).await {
            Ok(()) => {
                for item in reads {
                    let marked = Settled::Marked {
                        exchange: item.exchange().to_owned(),
                        read: true,
                    };
                    settled(item, marked);
                }
            }
            Err(failure) if failure.transient() => return Err(failure),
            Err(failure) => {
                for item in reads {
                    settled(item, Settled::Refused(failure.to_string()));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
