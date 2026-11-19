//! The account's session: signing in through the browser, starting from
//! a kept session, renaming, and signing out.

use std::sync::Arc;

use jiff::Timestamp;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use super::{Account, Connection, Show};
use crate::notifications::Notifier;
use crate::pictures::Pictures;
use crate::signin::{self, Pkce, Problem, Session};
use crate::sync;
use crate::sync::client::Client;
use crate::tray::Tray;

/// A session in use: its API, its pictures, and the task that follows
/// its feed.
pub(super) struct Running {
    pub(super) session: Session,
    pub(super) client: Client,
    pub(super) pictures: Pictures,
    /// Notified when the computer wakes or its network comes back, so
    /// the feed connects again at once.
    pub(super) woke: Arc<Notify>,
    /// When notifying began for this session: a one-way notification
    /// that arrived before it is not shown.
    pub(super) since: Timestamp,
    feed: JoinHandle<()>,
}

impl<N: Notifier, T: Tray> Account<N, T> {
    /// Checks the address the person typed, and sends their browser to
    /// its sign-in page.
    pub(super) async fn sign_in(&mut self, typed: &str) {
        self.problem = None;
        self.pending = None;
        let pkce = Pkce::new();
        let checked = match signin::server_url(typed) {
            Ok(server) => signin::check(&server, &pkce.challenge)
                .await
                .map(|()| server),
            Err(problem) => Err(problem),
        };
        match checked {
            Ok(server) => {
                self.preferences.server = Some(server.to_string());
                self.preferences.save(&self.places);
                let login = signin::login_url(&server, &pkce.challenge).to_string();
                tracing::debug!(%login, "sending the browser to sign in");
                self.show(Show::Browser(login));
                self.pending = Some((server, pkce));
            }
            Err(problem) => self.problem = Some(problem.to_string()),
        }
    }

    /// Finishes the sign-in the browser came back from: trades its code
    /// for a session, learns whose it is, and keeps it.
    pub(super) async fn callback(&mut self, callback: &str) {
        let signed_in = async {
            let code = signin::code_in(callback).ok_or(Problem::Unexpected)?;
            let (server, pkce) = self.pending.take().ok_or(Problem::Unexpected)?;
            let token = signin::redeem(&server, &code, &pkce.verifier).await?;
            let me = signin::name_of(&server, &token).await?;
            Ok::<Session, Problem>(Session { server, token, me })
        }
        .await;
        match signed_in {
            Ok(session) => {
                self.keep_session(&session).await;
                self.preferences.since = Some(Timestamp::now());
                self.preferences.save(&self.places);
                self.begin(session);
                self.show(Show::Inbox { token: None });
            }
            Err(problem) => self.problem = Some(problem.to_string()),
        }
    }

    async fn keep_session(&self, session: &Session) {
        let Some(keyring) = &self.keyring else { return };
        if let Err(error) = keyring.save(session).await {
            tracing::warn!(%error, "the session could not be kept, so the next start signs in again");
        }
    }

    /// Starts using `session`: follows its feed, and sends what the
    /// outbox kept.
    pub(super) fn begin(&mut self, session: Session) {
        let client = Client::new(&session.server, &session.token);
        let woke = Arc::new(Notify::new());
        let feed = tokio::spawn(sync::follow(
            client.clone(),
            session.me.clone(),
            self.places.device(),
            self.heard.clone(),
            woke.clone(),
        ));
        self.problem = None;
        self.running = Some(Running {
            pictures: Pictures::new(&self.places.cache, client.clone()),
            since: self.preferences.since.unwrap_or_else(Timestamp::now),
            client,
            session,
            woke,
            feed,
        });
    }

    /// Follows the feed under the person's new handle, and keeps the
    /// session under it.
    pub(super) async fn renamed(&mut self, me: String) {
        let Some(running) = self.running.take() else {
            return;
        };
        running.feed.abort();
        let session = Session {
            me,
            ..running.session
        };
        self.keep_session(&session).await;
        self.connection.connected = false;
        self.begin(session);
    }

    /// Signs out: forgets this device on the server, which ends the
    /// session that registered it, and forgets everything kept here.
    pub(super) async fn sign_out(&mut self) {
        if let Some(running) = &self.running {
            let _ = running
                .client
                .forget(&running.session.me, &self.places.device())
                .await;
            running.client.sign_out().await;
        }
        self.end(None).await;
    }

    /// Stops using the session and forgets what it brought here. The
    /// device id is kept, so the next sign-in registers the same device.
    pub(super) async fn end(&mut self, problem: Option<String>) {
        if let Some(running) = self.running.take() {
            running.feed.abort();
        }
        if let Some(keyring) = &self.keyring {
            keyring.clear().await;
        }
        for exchange in self.lost.keys() {
            self.notifier.withdraw(&format!("lost:{exchange}")).await;
        }
        self.outbox.clear();
        self.exchanges.clear();
        self.lost.clear();
        Pictures::clear(&self.places.cache);
        self.preferences.since = None;
        self.preferences.save(&self.places);
        self.connection = Connection::default();
        self.problem = problem;
    }
}
