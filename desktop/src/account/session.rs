//! The account's session: signing in through the browser, starting from
//! a kept session, renaming, signing out, and the server ending it.

use std::sync::Arc;

use jiff::Timestamp;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use super::{Account, Connection, Show};
use crate::notifications::{self, Notifier};
use crate::pictures::Pictures;
use crate::signin::{self, Pkce, Problem, Session};
use crate::sync;
use crate::sync::client::Client;
use crate::sync::outbox::Queued;
use crate::tray::Tray;

/// A session in use: its API, its pictures, and the task that follows
/// its feed.
pub(super) struct Running {
    pub(super) session: Session,
    /// Which of the account's sessions this is, as its client reports it
    /// when the server ends it.
    generation: u64,
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
                self.begin(session).await;
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
    /// outbox kept. Whatever said an earlier session was signed out is
    /// over.
    pub(super) async fn begin(&mut self, session: Session) {
        self.notifier.withdraw(notifications::SIGNED_OUT).await;
        self.generation += 1;
        let client = Client::new(&session.server, &session.token)
            .reporting(self.generation, self.heard.clone());
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
            generation: self.generation,
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
        self.begin(session).await;
    }

    /// Signs out: forgets this device on the server, which ends the
    /// session that registered it, and forgets everything kept here.
    pub(super) async fn sign_out(&mut self) {
        // Taken first, so the 401 that logging out of the session just
        // ended draws, and any other the session's calls draw from now
        // on, finds no session running to end.
        let running = self.running.take();
        if let Some(running) = &running {
            let _ = running
                .client
                .forget(&running.session.me, &self.places.device())
                .await;
            running.client.sign_out().await;
        }
        self.end(running, None).await;
    }

    /// The server ended session `generation`, for the reason its `code`
    /// names: the account signs out, drops what the outbox held, and
    /// says so in a notification. A report from a session that is no
    /// longer running changes nothing, so the many calls one ended
    /// session may have under way sign out once.
    pub(super) async fn ended(&mut self, generation: u64, code: Option<String>) {
        let running = self.running.as_ref();
        if running.is_none_or(|running| running.generation != generation) {
            return;
        }
        let queued = self.outbox.queued().iter();
        let dropped = queued
            .filter(|queued| matches!(queued, Queued::Reply { .. }))
            .count();
        let running = self.running.take();
        self.end(running, Some(Problem::Ended(code).to_string()))
            .await;
        if self.preferences.notifications {
            self.notifier.show(notifications::signed_out(dropped)).await;
        }
    }

    /// Stops using `running` and forgets what the session brought here.
    /// The device id and the server's address are kept, so the next
    /// sign-in registers the same device on the same server. The keyring
    /// is cleared last: until then, a run that stopped part way starts
    /// the next time with the session, and finds it ended.
    async fn end(&mut self, running: Option<Running>, problem: Option<String>) {
        if let Some(running) = running {
            running.feed.abort();
        }
        self.outbox.clear();
        for exchange in self.lost.keys() {
            self.notifier.withdraw(&format!("lost:{exchange}")).await;
        }
        self.exchanges.clear();
        self.lost.clear();
        Pictures::clear(&self.places.cache);
        self.preferences.since = None;
        self.preferences.save(&self.places);
        self.connection = Connection::default();
        self.problem = problem;
        if let Some(keyring) = &self.keyring {
            keyring.clear().await;
        }
    }
}
