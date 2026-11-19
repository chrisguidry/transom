//! `org.freedesktop.Notifications`, as GNOME Shell serves it: it keeps
//! what each notification said, and a test presses a button or dismisses
//! one as a person would.

// zbus's macro reads every argument of a D-Bus method, including those
// the method ignores, in code that no attribute on the method covers.
#![allow(clippy::used_underscore_binding)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use zbus::OwnedVariant;
use zbus::object_server::SignalEmitter;

/// The capabilities GNOME Shell 50 reports.
pub const GNOME: [&str; 6] = [
    "actions",
    "body",
    "body-markup",
    "icon-static",
    "persistence",
    "sound",
];

/// Why a notification closed, by the specification's numbers.
const DISMISSED: u32 = 2;
const CLOSED_BY_CALL: u32 = 3;

/// One notification as the app posted it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Posted {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    /// Each action's key and label, in the order given.
    pub actions: Vec<(String, String)>,
    /// The `urgency` hint: 0 low, 1 normal, 2 critical.
    pub urgency: u8,
    /// The `sound-name` hint, which GNOME Shell needs to make a sound.
    pub sound: Option<String>,
    pub desktop_entry: Option<String>,
    pub image_path: Option<String>,
    /// How many times this id was posted, so a test can tell an update.
    pub posts: u32,
}

#[derive(Default)]
struct State {
    next: u32,
    showing: Vec<Posted>,
}

/// The server's side that a test reads.
#[derive(Clone, Default)]
pub struct Notifications {
    state: Arc<Mutex<State>>,
    capabilities: Arc<Vec<String>>,
}

impl Notifications {
    pub(crate) fn with(capabilities: &[&str]) -> Self {
        Self {
            state: Arc::default(),
            capabilities: Arc::new(capabilities.iter().map(|&c| c.to_owned()).collect()),
        }
    }

    /// Every notification showing now, oldest first.
    ///
    /// # Panics
    ///
    /// Panics when a test panicked while it held the lock.
    #[must_use]
    pub fn showing(&self) -> Vec<Posted> {
        self.state
            .lock()
            .expect("no test poisons the lock")
            .showing
            .clone()
    }

    fn remove(&self, id: u32) -> bool {
        let mut state = self.state.lock().expect("no test poisons the lock");
        let before = state.showing.len();
        state.showing.retain(|posted| posted.id != id);
        state.showing.len() != before
    }
}

fn text(hints: &HashMap<String, OwnedVariant>, name: &str) -> Option<String> {
    hints
        .get(name)
        .and_then(|value| <&str>::try_from(value).ok())
        .map(str::to_owned)
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    fn get_capabilities(&self) -> Vec<String> {
        self.capabilities.to_vec()
    }

    // zbus calls every method of an interface on the object it serves.
    #[allow(clippy::unused_self)]
    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        ("test-desktop", "transom", "0.1.0", "1.2")
    }

    // `Notify` takes these eight arguments, and zbus hands each to the
    // method by value.
    #[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        _app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedVariant>,
        _expire_timeout: i32,
    ) -> u32 {
        let mut state = self.state.lock().expect("no test poisons the lock");
        let earlier = state
            .showing
            .iter()
            .position(|posted| replaces_id != 0 && posted.id == replaces_id);
        let (id, posts) = if let Some(index) = earlier {
            let earlier = state.showing.remove(index);
            (earlier.id, earlier.posts + 1)
        } else {
            state.next += 1;
            (state.next, 1)
        };
        state.showing.push(Posted {
            id,
            app_name,
            summary,
            body,
            actions: actions
                .as_chunks::<2>()
                .0
                .iter()
                .map(|[key, label]| (key.clone(), label.clone()))
                .collect(),
            urgency: hints
                .get("urgency")
                .and_then(|value| u8::try_from(value).ok())
                .unwrap_or(1),
            sound: text(&hints, "sound-name"),
            desktop_entry: text(&hints, "desktop-entry"),
            image_path: text(&hints, "image-path"),
            posts,
        });
        id
    }

    async fn close_notification(
        &self,
        id: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        if self.remove(id) {
            Self::notification_closed(&emitter, id, CLOSED_BY_CALL).await?;
        }
        Ok(())
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn activation_token(
        emitter: &SignalEmitter<'_>,
        id: u32,
        activation_token: &str,
    ) -> zbus::Result<()>;
}

impl Notifications {
    /// Presses `action` on notification `id` as GNOME Shell reports it:
    /// the activation token, the action, and then the close, since the
    /// Shell takes a notification down once it is acted on.
    pub(crate) async fn invoke(
        &self,
        emitter: &SignalEmitter<'_>,
        id: u32,
        action: &str,
    ) -> zbus::Result<()> {
        Self::activation_token(emitter, id, "test-activation-token").await?;
        Self::action_invoked(emitter, id, action).await?;
        self.dismiss(emitter, id).await
    }

    /// Dismisses notification `id`, as its close button does.
    pub(crate) async fn dismiss(&self, emitter: &SignalEmitter<'_>, id: u32) -> zbus::Result<()> {
        if self.remove(id) {
            Self::notification_closed(emitter, id, DISMISSED).await?;
        }
        Ok(())
    }
}
