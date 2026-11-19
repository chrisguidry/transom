//! Notifications through `org.freedesktop.Notifications`, the service
//! GNOME Shell, KDE Plasma, and most other desktops implement. It reports
//! a dismissal, which the app reads the exchange on, and it takes buttons
//! and body markup where the desktop's server says it has them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use tokio::sync::mpsc::UnboundedSender;
use zbus::Variant;

use super::{Action, Event, Notification, Notifier};
use crate::exchange::Urgency;

/// The desktop entry the notifications are filed under, so GNOME's
/// settings list them as transom's.
const DESKTOP_ENTRY: &str = "sh.transom";

/// Why the server closed a notification: the person dismissed it.
const DISMISSED: u32 = 2;

#[zbus::proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    fn get_capabilities(&self) -> zbus::Result<Vec<String>>;

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, Variant<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;

    fn close_notification(&self, id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn notification_closed(&self, id: u32, reason: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn action_invoked(&self, id: u32, action_key: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn activation_token(&self, id: u32, activation_token: String) -> zbus::Result<()>;
}

/// The server's id of each notification showing, by its tag. It is kept
/// in a file as well, so a run that follows a crash can take down what
/// the last one left up: those notifications' buttons reach no one.
#[derive(Debug, Default)]
struct Showing {
    path: PathBuf,
    ids: HashMap<String, u32>,
    /// The last few notifications the app took down itself. A person
    /// can press a button in the moment before the server hears of the
    /// withdrawal, and that press still names its notification here, so
    /// their answer is sent and they are told what became of it.
    withdrawn: Vec<(u32, String)>,
}

/// How many withdrawn notifications a late press can still name.
const WITHDRAWN_KEPT: usize = 16;

impl Showing {
    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let bytes = serde_json::to_vec(&self.ids).expect("the ids are plain data");
        let _ = std::fs::write(&self.path, bytes);
    }

    fn tag_of(&self, id: u32) -> Option<String> {
        self.ids
            .iter()
            .find(|(_, shown)| **shown == id)
            .map(|(tag, _)| tag.clone())
    }
}

pub struct Freedesktop {
    proxy: NotificationsProxy<'static>,
    showing: Arc<Mutex<Showing>>,
    actions: bool,
    markup: bool,
}

impl Freedesktop {
    /// Connects to the session's notification server, takes down what an
    /// earlier run left showing, and sends what people do with the
    /// notifications to `events`.
    pub async fn open(
        connection: &zbus::Connection,
        state: &Path,
        events: UnboundedSender<Event>,
    ) -> zbus::Result<Self> {
        let proxy = NotificationsProxy::new(connection).await?;
        let capabilities = proxy.get_capabilities().await.unwrap_or_default();
        let has = |capability: &str| capabilities.iter().any(|found| found == capability);
        let path = state.join("notified.json");
        let left: HashMap<String, u32> = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        for id in left.values() {
            let _ = proxy.close_notification(*id).await;
        }
        let showing = Arc::new(Mutex::new(Showing {
            path,
            ids: HashMap::new(),
            withdrawn: Vec::new(),
        }));
        showing.lock().expect("the lock is never poisoned").save();
        // Subscribed before the first notification is shown, so no press
        // on one is missed.
        let signals = proxy.inner().receive_all_signals().await?;
        tokio::spawn(listen(signals, showing.clone(), events));
        Ok(Self {
            actions: has("actions"),
            markup: has("body-markup"),
            proxy,
            showing,
        })
    }

    fn body(&self, notification: &Notification) -> String {
        let (summary, body) = (&notification.summary, &notification.body);
        if self.markup {
            let body = if body.is_empty() {
                String::new()
            } else {
                format!(" {}", escape(body))
            };
            format!("<b>{}</b>{body}", escape(summary))
        } else if body.is_empty() {
            summary.clone()
        } else {
            format!("{summary}: {body}")
        }
    }
}

/// `text` with the characters that begin markup written as entities.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

impl Notifier for Freedesktop {
    async fn show(&self, notification: Notification) {
        let replaces = {
            let mut showing = self.showing.lock().expect("the lock is never poisoned");
            let earlier = notification
                .replaces
                .as_ref()
                .and_then(|tag| showing.ids.remove(tag));
            showing.ids.get(&notification.tag).copied().or(earlier)
        };
        let mut actions = vec!["default".to_owned(), "Open".to_owned()];
        if self.actions {
            for (action, label) in &notification.buttons {
                actions.push(action.key());
                actions.push(label.clone());
            }
        }
        let actions: Vec<&str> = actions.iter().map(String::as_str).collect();
        let urgency: u8 = match notification.urgency {
            Urgency::VeryLow | Urgency::Low => 0,
            Urgency::Normal | Urgency::High => 1,
            Urgency::Urgent => 2,
        };
        let mut hints = HashMap::from([
            ("desktop-entry", Variant::from(DESKTOP_ENTRY)),
            ("urgency", Variant::from(urgency)),
        ]);
        // GNOME Shell sounds only when a notification names a sound, so
        // a quiet one names none.
        if notification.alert && notification.urgency >= Urgency::Normal {
            hints.insert("sound-name", Variant::from("message-new-instant"));
        }
        let icon = notification
            .icon
            .as_ref()
            .and_then(|icon| icon.to_str())
            .unwrap_or(DESKTOP_ENTRY);
        if notification.icon.is_some() {
            hints.insert("image-path", Variant::from(icon));
        }
        let body = self.body(&notification);
        let posted = self
            .proxy
            .notify(
                "transom",
                replaces.unwrap_or(0),
                DESKTOP_ENTRY,
                &notification.title,
                &body,
                &actions,
                hints,
                -1,
            )
            .await;
        match posted {
            Ok(id) => {
                let mut showing = self.showing.lock().expect("the lock is never poisoned");
                showing.ids.insert(notification.tag, id);
                showing.save();
            }
            Err(error) => tracing::warn!(%error, "a notification could not be shown"),
        }
    }

    async fn withdraw(&self, tag: &str) {
        let id = {
            let mut showing = self.showing.lock().expect("the lock is never poisoned");
            let id = showing.ids.remove(tag);
            if let Some(id) = id {
                showing.withdrawn.push((id, tag.to_owned()));
                let excess = showing.withdrawn.len().saturating_sub(WITHDRAWN_KEPT);
                showing.withdrawn.drain(..excess);
            }
            showing.save();
            id
        };
        if let Some(id) = id {
            let _ = self.proxy.close_notification(id).await;
        }
    }
}

/// Passes on what people do with the notifications, which the server
/// says in `signals`. The server sends a button's activation token just
/// before the button itself, and closes the notification just after, so
/// a press is reported once, as the press, and the close that follows it
/// is not a dismissal. The three signals are read from one stream, in
/// the order the server sent them: a stream for each would let the close
/// overtake the press.
async fn listen(
    mut signals: zbus::proxy::SignalStream<'static>,
    showing: Arc<Mutex<Showing>>,
    events: UnboundedSender<Event>,
) {
    // Takes the notification with the server's `id` out of what is
    // showing, and returns its tag.
    let take = |id: u32| {
        let mut showing = showing.lock().expect("the lock is never poisoned");
        let tag = showing.tag_of(id)?;
        showing.ids.remove(&tag);
        showing.save();
        Some(tag)
    };
    let mut token: Option<(u32, String)> = None;
    while let Some(signal) = signals.next().await {
        let header = signal.header();
        let body = signal.body();
        match header.member().map(zbus::names::MemberName::as_str) {
            Some("ActivationToken") => token = body.deserialize::<(u32, String)>().ok(),
            Some("ActionInvoked") => {
                let Ok((id, key)) = body.deserialize::<(u32, String)>() else {
                    continue;
                };
                let token = token
                    .take()
                    .filter(|(for_id, _)| *for_id == id)
                    .map(|(_, token)| token);
                let pressed = take(id).or_else(|| {
                    let mut showing = showing.lock().expect("the lock is never poisoned");
                    let late = showing.withdrawn.iter().position(|(was, _)| *was == id)?;
                    Some(showing.withdrawn.remove(late).1)
                });
                if let (Some(tag), Some(action)) = (pressed, Action::from_key(&key)) {
                    let _ = events.send(Event::Invoked { tag, action, token });
                }
            }
            Some("NotificationClosed") => {
                let Ok((id, reason)) = body.deserialize::<(u32, u32)>() else {
                    continue;
                };
                if let Some(tag) = take(id)
                    && reason == DISMISSED
                {
                    let _ = events.send(Event::Dismissed { tag });
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
