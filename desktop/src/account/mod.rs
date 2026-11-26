//! The signed-in account as one task: it owns the session, the copy of
//! the inbox, and the outbox, hears the feed, the notifications, the tray,
//! and the window, and after each thing it hears makes the window's
//! snapshot, the notifications, and the tray's menu say the same thing.

pub mod arguments;
mod delivery;
pub mod desktop;
mod publish;
mod session;

use std::collections::HashMap;
use std::sync::Arc;

use jiff::Timestamp;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;
use url::Url;

use crate::exchange::Exchange;
use crate::inbox::{Entry, Sending};
use crate::notifications::{self, Notifier, Posted};
use crate::pictures::Pictures;
use crate::settings::{Places, Preferences};
use crate::signin::Pkce;
use crate::signin::keyring::Keyring;
use crate::sync::Heard;
use crate::sync::client::Client;
use crate::sync::outbox::{Origin, Outbox, Queued};
use crate::tray::{self, Menu, Tray};
use delivery::Delivery;
use session::Running;

/// What the window asks of the account.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Begin signing in to the server at this address, as typed.
    SignIn(String),
    /// The desktop started the app with the sign-in's callback.
    Callback(String),
    Reply {
        exchange: String,
        choice: Option<String>,
        text: Option<String>,
    },
    Mark {
        exchanges: Vec<String>,
        read: bool,
    },
    Withdraw(String),
    /// The person has seen that their answer to this exchange was not
    /// used.
    Seen(String),
    /// The person renamed themselves to this handle.
    Renamed(String),
    Notifications(bool),
    /// The computer woke or its network came back.
    Woke,
    SignOut,
}

/// What the account asks of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Show {
    Inbox {
        token: Option<String>,
    },
    Exchange {
        id: String,
        token: Option<String>,
        /// Whether to put the cursor in the reply's text box.
        reply: bool,
    },
    /// Open this address in the person's browser.
    Browser(String),
    /// Say this once, such as why the server refused something.
    Notice(String),
    Quit,
}

/// Whether anyone is signed in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    /// The kept session has not been looked for yet: the window's
    /// snapshot before the account sends its first.
    #[default]
    Starting,
    SignedOut {
        problem: Option<String>,
        /// Whether a sign-in is under way in the browser.
        waiting: bool,
    },
    SignedIn {
        server: Url,
        me: String,
    },
}

/// Everything the window draws from, replaced whole after each change.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub phase: Phase,
    pub entries: Vec<Entry>,
    /// Whether the server's list has arrived since the app started.
    pub loaded: bool,
    pub connected: bool,
    /// When the feed last said anything.
    pub heard: Option<Timestamp>,
    pub preferences: Preferences,
    /// The signed-in session's API and pictures, for what a screen asks
    /// of the server itself.
    pub client: Option<Client>,
    pub pictures: Option<Pictures>,
}

impl Snapshot {
    /// The signed-in handle, with its `@`.
    pub fn me(&self) -> Option<&str> {
        match &self.phase {
            Phase::SignedIn { me, .. } => Some(me),
            _ => None,
        }
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.exchange.id == id)
    }
}

/// The channels the account hears on.
pub struct Hearing {
    pub commands: UnboundedReceiver<Command>,
    pub notified: UnboundedReceiver<notifications::Event>,
    pub picked: UnboundedReceiver<tray::Picked>,
}

pub struct Account<N, T> {
    places: Places,
    preferences: Preferences,
    keyring: Option<Keyring>,
    notifier: N,
    tray: T,
    snapshots: watch::Sender<Arc<Snapshot>>,
    shows: UnboundedSender<Show>,
    heard: UnboundedSender<Heard>,
    settled: UnboundedSender<Delivery>,
    running: Option<Running>,
    /// The generation of the session `begin` last started. Each session
    /// has its own, so a report that a session ended names which one.
    generation: u64,
    /// The sign-in under way in the browser: its server and PKCE pair.
    pending: Option<(Url, Pkce)>,
    problem: Option<String>,
    exchanges: HashMap<String, Exchange>,
    outbox: Outbox,
    /// Each answer made here that came after someone else's, until the
    /// person has seen that.
    lost: HashMap<String, Sending>,
    posted: Posted,
    menu: Option<Menu>,
    connection: Connection,
    delivering: bool,
}

/// What the feed has said of the server.
#[derive(Default)]
struct Connection {
    /// Whether the server's list has arrived.
    loaded: bool,
    connected: bool,
    /// When the feed last said anything.
    last_heard: Option<Timestamp>,
}

impl<N: Notifier, T: Tray> Account<N, T> {
    /// Runs the account until the person quits. It first looks for a
    /// session kept from an earlier run.
    pub async fn run(
        places: Places,
        keyring: Option<Keyring>,
        notifier: N,
        tray: T,
        snapshots: watch::Sender<Arc<Snapshot>>,
        shows: UnboundedSender<Show>,
        mut hearing: Hearing,
    ) {
        let (heard, mut feed) = tokio::sync::mpsc::unbounded_channel();
        let (settled, mut deliveries) = tokio::sync::mpsc::unbounded_channel();
        let mut account = Self {
            preferences: Preferences::load(&places),
            outbox: Outbox::open(&places.state.join("outbox.json")),
            places,
            keyring,
            notifier,
            tray,
            snapshots,
            shows,
            heard,
            settled,
            running: None,
            generation: 0,
            pending: None,
            problem: None,
            exchanges: HashMap::new(),
            lost: HashMap::new(),
            posted: Posted::default(),
            menu: None,
            connection: Connection::default(),
            delivering: false,
        };
        let kept = match &account.keyring {
            Some(keyring) => keyring.load().await,
            None => None,
        };
        if let Some(kept) = kept {
            account.begin(kept).await;
        }
        account.publish().await;
        loop {
            // The account holds a sender of the feed's channel and of
            // the outbox's, so there is always a branch to wait on.
            tokio::select! {
                Some(command) = hearing.commands.recv() => account.command(command).await,
                Some(event) = hearing.notified.recv() => account.notified(event),
                Some(picked) = hearing.picked.recv() => {
                    if !account.picked(picked) {
                        return;
                    }
                }
                Some(heard) = feed.recv() => account.heard(heard).await,
                Some(delivery) = deliveries.recv() => account.delivered(delivery).await,
            }
            account.publish().await;
        }
    }

    fn me(&self) -> Option<&str> {
        self.running
            .as_ref()
            .map(|running| running.session.me.as_str())
    }

    fn show(&self, show: Show) {
        let _ = self.shows.send(show);
    }

    async fn command(&mut self, command: Command) {
        match command {
            Command::SignIn(typed) => self.sign_in(&typed).await,
            Command::Callback(callback) => self.callback(&callback).await,
            Command::Reply {
                exchange,
                choice,
                text,
            } => self.reply(exchange, Sending { choice, text }, Origin::Window),
            Command::Mark { exchanges, read } => self.mark(exchanges, read),
            Command::Withdraw(exchange) => self.withdraw(&exchange).await,
            Command::Seen(exchange) => {
                if self.lost.remove(&exchange).is_some() {
                    self.notifier.withdraw(&format!("lost:{exchange}")).await;
                }
            }
            Command::Renamed(me) => self.renamed(me).await,
            Command::Notifications(on) => self.notifications(on),
            Command::Woke => {
                if let Some(running) = &self.running {
                    running.woke.notify_one();
                }
                self.deliver();
            }
            Command::SignOut => self.sign_out().await,
        }
    }

    fn notifications(&mut self, on: bool) {
        self.preferences.notifications = on;
        self.preferences.save(&self.places);
    }

    fn reply(&mut self, exchange: String, sending: Sending, origin: Origin) {
        self.outbox.put(Queued::Reply {
            exchange,
            sending,
            origin,
        });
        self.deliver();
    }

    fn mark(&mut self, exchanges: Vec<String>, read: bool) {
        for exchange in exchanges {
            self.outbox.put(Queued::Read { exchange, read });
        }
        self.deliver();
    }

    async fn withdraw(&mut self, exchange: &str) {
        let Some(running) = &self.running else { return };
        if let Err(failure) = running.client.withdraw(exchange).await {
            self.show(Show::Notice(failure.to_string()));
        }
    }

    fn notified(&mut self, event: notifications::Event) {
        use notifications::{Action, Event, SIGNED_OUT};
        match event {
            Event::Invoked { tag, token, .. } if tag == SIGNED_OUT => {
                self.show(Show::Inbox { token });
            }
            Event::Dismissed { tag } if tag == SIGNED_OUT => {}
            Event::Dismissed { tag } => {
                if let Some(exchange) = tag.strip_prefix("lost:") {
                    self.lost.remove(exchange);
                } else {
                    self.posted.handled(&tag);
                    self.mark(vec![tag], true);
                }
            }
            Event::Invoked { tag, action, token } => {
                if let Some(exchange) = tag.strip_prefix("lost:") {
                    self.show(Show::Exchange {
                        id: exchange.to_owned(),
                        token,
                        reply: false,
                    });
                    return;
                }
                self.posted.handled(&tag);
                match action {
                    Action::GotIt => self.mark(vec![tag], true),
                    Action::Choice(key) => self.reply(
                        tag,
                        Sending {
                            choice: Some(key),
                            text: None,
                        },
                        Origin::Notification,
                    ),
                    Action::Answer | Action::Open => self.show(Show::Exchange {
                        id: tag,
                        token,
                        reply: false,
                    }),
                    Action::Reply => self.show(Show::Exchange {
                        id: tag,
                        token,
                        reply: true,
                    }),
                    Action::Link(href) => {
                        self.show(Show::Browser(href));
                        self.mark(vec![tag], true);
                    }
                }
            }
        }
    }

    /// Does what the person picked in the tray, and returns whether the
    /// account goes on running.
    fn picked(&mut self, picked: tray::Picked) -> bool {
        use tray::Picked;
        match picked {
            Picked::Choice { exchange, key } => self.reply(
                exchange,
                Sending {
                    choice: Some(key),
                    text: None,
                },
                Origin::Tray,
            ),
            Picked::Reply { exchange, token } => self.show(Show::Exchange {
                id: exchange,
                token,
                reply: true,
            }),
            Picked::Open { exchange, token } => self.show(Show::Exchange {
                id: exchange,
                token,
                reply: false,
            }),
            Picked::Inbox { token } => self.show(Show::Inbox { token }),
            Picked::Notifications(on) => self.notifications(on),
            Picked::Quit => {
                self.show(Show::Quit);
                return false;
            }
        }
        true
    }

    async fn heard(&mut self, heard: Heard) {
        match heard {
            Heard::Listed(listed) => {
                self.exchanges = listed
                    .into_iter()
                    .map(|exchange| (exchange.id.clone(), exchange))
                    .collect();
                self.lost.retain(|id, _| self.exchanges.contains_key(id));
                self.connection.connected = true;
                self.connection.last_heard = Some(Timestamp::now());
                self.deliver();
            }
            Heard::Changed(exchange) => {
                self.connection.last_heard = Some(Timestamp::now());
                self.keep(*exchange);
            }
            Heard::Gone(id) => {
                self.connection.last_heard = Some(Timestamp::now());
                self.exchanges.remove(&id);
            }
            Heard::Disconnected => self.connection.connected = false,
            Heard::SignedOut { generation, code } => self.ended(generation, code).await,
        }
    }

    /// Keeps `exchange` in place of the copy of it. `GET /exchanges/{id}`
    /// and a reply's answer give no read state, so the copy's is kept.
    fn keep(&mut self, mut exchange: Exchange) {
        if exchange.read.is_none() {
            exchange.read = self.exchanges.get(&exchange.id).and_then(|kept| kept.read);
        }
        self.exchanges.insert(exchange.id.clone(), exchange);
    }
}

#[cfg(test)]
mod tests;
