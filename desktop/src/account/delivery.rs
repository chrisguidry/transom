//! The account's outbox: sending what the person did here, and keeping
//! what the server said of each thing sent.

use super::{Account, Show};
use crate::exchange::{State, describe};
use crate::inbox::{Entry, Sending};
use crate::notifications::{self, Notifier};
use crate::sync::outbox::{self, Origin, Queued, Settled};
use crate::tray::Tray;

/// What a run of the outbox reports.
#[derive(Debug)]
pub(super) enum Delivery {
    Settled(Queued, Settled),
    /// The run ended, with whether everything it took was settled.
    Finished(bool),
}

impl<N: Notifier, T: Tray> Account<N, T> {
    /// Starts sending the outbox, unless a run is already sending it or
    /// there is nothing to send.
    pub(super) fn deliver(&mut self) {
        let Some(running) = &self.running else { return };
        if self.delivering || self.outbox.queued().is_empty() {
            return;
        }
        self.delivering = true;
        let (client, me) = (running.client.clone(), running.session.me.clone());
        let items = self.outbox.queued().to_vec();
        let settled = self.settled.clone();
        tokio::spawn(async move {
            let report = settled.clone();
            let finished = outbox::deliver(&client, &me, items, move |item, outcome| {
                let _ = report.send(Delivery::Settled(item, outcome));
            })
            .await;
            let _ = settled.send(Delivery::Finished(finished.is_ok()));
        });
    }

    pub(super) async fn delivered(&mut self, delivery: Delivery) {
        match delivery {
            Delivery::Finished(whole) => {
                self.delivering = false;
                // What was queued during the run goes in the next one. A
                // run that stopped short waits for the feed to connect.
                if whole {
                    self.deliver();
                }
            }
            Delivery::Settled(item, outcome) => {
                self.outbox.sent(&item);
                match outcome {
                    Settled::Accepted(exchange) => {
                        let mut exchange = *exchange;
                        exchange.read = exchange.read.or(Some(true));
                        self.keep(exchange);
                    }
                    Settled::Lost {
                        exchange,
                        sending,
                        origin,
                        winner,
                    } => self.lose(exchange, sending, origin, *winner).await,
                    Settled::Marked { exchange, read } => {
                        if let Some(kept) = self.exchanges.get_mut(&exchange)
                            && kept.read.is_some()
                        {
                            kept.read = Some(read);
                        }
                    }
                    Settled::Refused(reason) => self.show(Show::Notice(reason)),
                }
            }
        }
    }

    /// Records that `sending`, an answer to `exchange`, came after
    /// `winner`'s, and says so in a notification when the answer was made
    /// where no window shows it.
    async fn lose(
        &mut self,
        exchange: String,
        sending: Sending,
        origin: Origin,
        winner: crate::exchange::Reply,
    ) {
        // The exchange may have left the list since the answer was made.
        let kept = self.exchanges.get_mut(&exchange);
        let Some(kept) = kept else { return };
        kept.state = State::Replied;
        kept.reply = Some(winner.clone());
        let said = describe::lost(kept, &sending, &winner);
        let entry = Entry {
            exchange: kept.clone(),
            read: kept.read,
            sending: None,
            lost: Some(sending.clone()),
        };
        self.lost.insert(exchange, sending);
        if origin != Origin::Window && self.preferences.notifications {
            let mut notification = notifications::lost(&entry, said);
            self.picture(&mut notification).await;
            self.notifier.show(notification).await;
        }
    }
}
