//! What the account shows of itself: the window's snapshot, the
//! notifications, and the tray's menu, each made from what it holds.

use std::sync::Arc;

use jiff::Timestamp;

use super::{Account, Phase, Snapshot};
use crate::inbox::{Entry, Inbox};
use crate::notifications::{self, Notifier, Step};
use crate::tray::{Menu, Tray};

impl<N: Notifier, T: Tray> Account<N, T> {
    fn entries(&self) -> Vec<Entry> {
        self.exchanges
            .values()
            .map(|exchange| {
                let sending = self.outbox.reply_to(&exchange.id).cloned();
                let read = exchange.read.map(|read| {
                    self.outbox
                        .marked(&exchange.id)
                        .unwrap_or(sending.is_some() || read)
                });
                Entry {
                    exchange: exchange.clone(),
                    read,
                    sending,
                    lost: self.lost.get(&exchange.id).cloned(),
                }
            })
            .collect()
    }

    /// Fetches the picture `notification` names, giving up after a few
    /// seconds, since a notification late is worse than one without its
    /// picture.
    pub(super) async fn picture(&self, notification: &mut notifications::Notification) {
        let (Some(running), Some(url)) = (&self.running, &notification.picture) else {
            return;
        };
        let fetch = running.pictures.picture(url);
        notification.icon = tokio::time::timeout(std::time::Duration::from_secs(3), fetch)
            .await
            .ok()
            .flatten();
    }

    /// Makes the window's snapshot, the notifications, and the tray say
    /// what the account now holds.
    pub(super) async fn publish(&mut self) {
        let entries = self.entries();
        let now = Timestamp::now();
        let me = self.me().map(str::to_owned);
        let steps = match (&me, &self.running) {
            (Some(me), Some(running))
                if self.preferences.notifications
                    && (self.connection.connected || self.connection.loaded) =>
            {
                // The first list since the app started posts what waits
                // without a sound.
                let first = !self.connection.loaded;
                self.posted.plan(&entries, me, now, running.since, first)
            }
            (Some(_), Some(_)) if self.preferences.notifications => Vec::new(),
            _ => self
                .posted
                .clear()
                .into_iter()
                .map(Step::Withdraw)
                .collect(),
        };
        if self.connection.connected {
            self.connection.loaded = true;
        }
        for step in steps {
            match step {
                Step::Show(mut notification) => {
                    self.picture(&mut notification).await;
                    self.notifier.show(notification).await;
                }
                Step::Withdraw(tag) => self.notifier.withdraw(&tag).await,
            }
        }
        let menu = match &me {
            Some(me) => Menu::of(
                me,
                &entries,
                &Inbox::of(me, &entries, now),
                self.preferences.notifications,
                self.connection.connected,
            ),
            None => Menu::default(),
        };
        if self.menu.as_ref() != Some(&menu) {
            self.tray.show(menu.clone()).await;
            self.menu = Some(menu);
        }
        let phase = match &self.running {
            Some(running) => Phase::SignedIn {
                server: running.session.server.clone(),
                me: running.session.me.clone(),
            },
            None => Phase::SignedOut {
                problem: self.problem.clone(),
                waiting: self.pending.is_some(),
            },
        };
        self.snapshots.send_replace(Arc::new(Snapshot {
            phase,
            entries,
            loaded: self.connection.loaded,
            connected: self.connection.connected,
            heard: self.connection.last_heard,
            preferences: self.preferences.clone(),
            client: self.running.as_ref().map(|running| running.client.clone()),
            pictures: self
                .running
                .as_ref()
                .map(|running| running.pictures.clone()),
        }));
    }
}
