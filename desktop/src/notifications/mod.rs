//! Which notifications the desktop should show, decided from the inbox.
//! A notification is one view of an exchange: it is posted when an
//! exchange this person received is open and unread, and withdrawn when
//! the exchange ends, is read on any device, or leaves the inbox.

pub mod freedesktop;

use std::collections::{HashMap, HashSet};
use std::future::Future;

use jiff::Timestamp;

use crate::exchange::describe;
use crate::exchange::{Exchange, State, Urgency};
use crate::inbox::Entry;

/// The most buttons a notification carries. GNOME Shell shows three and
/// drops the rest.
const BUTTONS: usize = 3;

/// What a button, or a click on the notification itself, asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Read it, and stop an urgent message repeating.
    GotIt,
    /// Reply with the choice of this key.
    Choice(String),
    /// Open the exchange, to choose among more choices than fit.
    Answer,
    /// Open the exchange with its text box ready.
    Reply,
    /// Open this address in the browser.
    Link(String),
    /// Open the exchange: a click on the notification's body.
    Open,
}

impl Action {
    /// The action as the notification server passes it back.
    pub fn key(&self) -> String {
        match self {
            Self::GotIt => "got-it".to_owned(),
            Self::Choice(key) => format!("choice:{key}"),
            Self::Answer => "answer".to_owned(),
            Self::Reply => "reply".to_owned(),
            Self::Link(href) => format!("link:{href}"),
            Self::Open => "default".to_owned(),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "got-it" => Self::GotIt,
            "answer" => Self::Answer,
            "reply" => Self::Reply,
            "default" => Self::Open,
            _ => match key.split_once(':')? {
                ("choice", key) => Self::Choice(key.to_owned()),
                ("link", href) => Self::Link(href.to_owned()),
                _ => return None,
            },
        })
    }
}

/// One notification, as any desktop could show it.
#[derive(Clone, Debug, PartialEq)]
pub struct Notification {
    /// What the notification is about: an exchange's id, or `lost:` and
    /// the id for an answer that was not used.
    pub tag: String,
    /// The sender.
    pub title: String,
    /// The summary, and the start of the body under it.
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    /// Whether to sound and show a banner again, or to change what is
    /// already shown without either.
    pub alert: bool,
    pub buttons: Vec<(Action, String)>,
    /// The sender's picture, as the server gives its URL.
    pub picture: Option<String>,
    /// A file holding that picture, once it has been fetched.
    pub icon: Option<std::path::PathBuf>,
    /// The tag of a notification this one takes the place of.
    pub replaces: Option<String>,
}

/// What a person did with a notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// They pressed a button or the notification itself. `token` lets a
    /// window take the focus on their behalf.
    Invoked {
        tag: String,
        action: Action,
        token: Option<String>,
    },
    /// They dismissed it.
    Dismissed { tag: String },
}

/// The desktop's notifications. The app has one implementation, the
/// freedesktop notification service, and this is where a second would
/// fit, such as the desktop portal's.
pub trait Notifier: Send + Sync + 'static {
    /// Shows `notification`, in place of the one with the same tag if
    /// one is showing.
    fn show(&self, notification: Notification) -> impl Future<Output = ()> + Send;

    /// Takes down the notification with `tag`, if one is showing.
    fn withdraw(&self, tag: &str) -> impl Future<Output = ()> + Send;
}

/// A desktop without a notification service shows nothing.
impl<N: Notifier> Notifier for Option<N> {
    async fn show(&self, notification: Notification) {
        if let Some(notifier) = self {
            notifier.show(notification).await;
        }
    }

    async fn withdraw(&self, tag: &str) {
        if let Some(notifier) = self {
            notifier.withdraw(tag).await;
        }
    }
}

/// The buttons of `exchange`'s notification, three at most, by what the
/// message asks. An urgent message leads with Got it. A question with a
/// prompt has Reply…, and its choices beside it when all of them fit;
/// without a prompt, each choice is a button, or Answer… opens the
/// exchange when they do not all fit. Then the links, each by its title.
/// An alert whose choices sit beside links leads with its first link.
pub fn buttons(exchange: &Exchange) -> Vec<(Action, String)> {
    let message = &exchange.message;
    let mut buttons = Vec::new();
    if message.urgency == Urgency::Urgent {
        buttons.push((Action::GotIt, "Got it".to_owned()));
    }
    let mut links: Vec<(Action, String)> = message
        .links
        .iter()
        .map(|link| (Action::Link(link.href.clone()), link.label().to_owned()))
        .collect();
    if !links.is_empty() && !message.choices.is_empty() && message.prompt.is_none() {
        buttons.push(links.remove(0));
    }
    let choices: Vec<(Action, String)> = message
        .choices
        .iter()
        .map(|choice| {
            (
                Action::Choice(choice.key().to_owned()),
                choice.label.clone(),
            )
        })
        .collect();
    let room = BUTTONS.saturating_sub(buttons.len());
    if message.prompt.is_some() {
        buttons.push((Action::Reply, "Reply…".to_owned()));
        if choices.len() < room {
            buttons.extend(choices);
        }
    } else if choices.len() <= room {
        buttons.extend(choices);
    } else {
        buttons.push((Action::Answer, "Answer…".to_owned()));
    }
    buttons.extend(links);
    buttons.truncate(BUTTONS);
    buttons
}

fn notification(exchange: &Exchange, alert: bool, replaces: Option<String>) -> Notification {
    Notification {
        tag: exchange.id.clone(),
        title: exchange.sender().to_owned(),
        summary: describe::title(exchange),
        body: describe::excerpt(exchange),
        urgency: exchange.message.urgency,
        alert,
        buttons: buttons(exchange),
        picture: exchange.from_picture.clone(),
        icon: None,
        replaces,
    }
}

/// The notification that says an answer made outside a window was not
/// used.
pub fn lost(entry: &Entry, said: String) -> Notification {
    Notification {
        tag: format!("lost:{}", entry.exchange.id),
        title: entry.exchange.sender().to_owned(),
        summary: describe::title(&entry.exchange),
        body: said,
        urgency: Urgency::Normal,
        alert: true,
        buttons: Vec::new(),
        picture: entry.exchange.from_picture.clone(),
        icon: None,
        replaces: None,
    }
}

/// What to do to make the desktop's notifications what the inbox says.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Show(Notification),
    Withdraw(String),
}

/// What was posted, so each exchange alerts once, and a notification is
/// taken down when its exchange no longer calls for one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Posted {
    /// Each exchange with a notification showing, and how many of an
    /// urgent message's repeats it has alerted for.
    showing: HashMap<String, u32>,
    /// Each exchange whose notification this person dismissed or acted
    /// on here, which is not posted again while the read mark is on its
    /// way to the server.
    handled: HashSet<String>,
}

impl Posted {
    /// Records that the person dismissed or acted on `exchange`'s
    /// notification, so the desktop has already taken it down.
    pub fn handled(&mut self, exchange: &str) {
        self.showing.remove(exchange);
        self.handled.insert(exchange.to_owned());
    }

    /// Forgets everything, as signing out does, and returns the tags
    /// still showing.
    pub fn clear(&mut self) -> Vec<String> {
        self.handled.clear();
        self.showing.drain().map(|(tag, _)| tag).collect()
    }

    /// The steps that bring the notifications in line with `entries`, as
    /// `me` reads them at `now`. `since` is when notifying began for this
    /// session: a one-way notification that arrived before it is not
    /// posted, and `first` says the list is the first since the app
    /// started, when nothing sounds.
    pub fn plan(
        &mut self,
        entries: &[Entry],
        me: &str,
        now: Timestamp,
        since: Timestamp,
        first: bool,
    ) -> Vec<Step> {
        let mut steps = Vec::new();
        let wanted: HashMap<&str, &Entry> = entries
            .iter()
            .filter(|entry| shown(entry, me, now, since))
            .map(|entry| (entry.exchange.id.as_str(), entry))
            .collect();
        self.handled
            .retain(|exchange| wanted.contains_key(exchange.as_str()));
        // The feed says a message was replaced before it sends the
        // message that replaced it. Until that one is in the list, the
        // replaced message keeps its notification, for the new message
        // to take over.
        let replacing: HashSet<&str> = entries
            .iter()
            .filter_map(|entry| entry.exchange.replaces.as_deref())
            .collect();
        let awaits_its_replacement = |entry: &Entry| {
            let exchange = &entry.exchange;
            self.showing.contains_key(&exchange.id)
                && exchange.state == State::Replaced
                && !replacing.contains(exchange.id.as_str())
        };
        let awaiting: HashSet<&str> = entries
            .iter()
            .filter(|entry| awaits_its_replacement(entry))
            .map(|entry| entry.exchange.id.as_str())
            .collect();
        let mut gone: Vec<String> = self
            .showing
            .keys()
            .filter(|exchange| !wanted.contains_key(exchange.as_str()))
            .filter(|exchange| !awaiting.contains(exchange.as_str()))
            .cloned()
            .collect();
        // A message that replaces one still showing takes over its
        // notification, so the earlier one is not withdrawn first.
        let mut taken_over = HashSet::new();
        let mut entries: Vec<&&Entry> = wanted.values().collect();
        entries.sort_by_key(|entry| entry.exchange.created);
        for entry in entries {
            let exchange = &entry.exchange;
            if self.handled.contains(&exchange.id) {
                continue;
            }
            let quiet = exchange.message.urgency <= Urgency::Low;
            match self.showing.get(&exchange.id) {
                Some(repeats) if *repeats >= exchange.repeats => {}
                Some(_) => {
                    self.showing.insert(exchange.id.clone(), exchange.repeats);
                    steps.push(Step::Show(notification(exchange, true, None)));
                }
                None => {
                    let replaces = exchange
                        .replaces
                        .clone()
                        .filter(|earlier| self.showing.contains_key(earlier));
                    if let Some(earlier) = &replaces {
                        taken_over.insert(earlier.clone());
                        self.showing.remove(earlier);
                    }
                    self.showing.insert(exchange.id.clone(), exchange.repeats);
                    let alert = !first && !quiet;
                    steps.push(Step::Show(notification(exchange, alert, replaces)));
                }
            }
        }
        gone.retain(|exchange| !taken_over.contains(exchange));
        gone.sort();
        for exchange in gone {
            self.showing.remove(&exchange);
            steps.push(Step::Withdraw(exchange));
        }
        steps
    }
}

/// Whether `entry` calls for a notification: an exchange this person
/// received and did not send, open, unread, and before its deadline,
/// that is not `very-low`. That is every such question, and each one-way
/// notification that arrived since notifying began.
fn shown(entry: &Entry, me: &str, now: Timestamp, since: Timestamp) -> bool {
    let exchange = &entry.exchange;
    exchange.from != me
        && exchange.state == State::Open
        && entry.read == Some(false)
        && entry.sending.is_none()
        && exchange.message.urgency != Urgency::VeryLow
        && exchange.expires.is_none_or(|expires| expires > now)
        && (exchange.question || exchange.created >= since)
}

#[cfg(test)]
mod tests;
