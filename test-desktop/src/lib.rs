//! The parts of a desktop that transom's desktop app talks to, for its
//! tests: a notification server, a tray host's registry, and a keyring,
//! each speaking the real D-Bus protocol on whatever bus it is given. A
//! test runs them on a `TestBus`, a private bus, so nothing reaches the
//! desktop of whoever runs the tests. `TestSystem` does the same for the
//! two services of the system bus the app listens to.
//!
//! ```no_run
//! # async fn example() -> zbus::Result<()> {
//! let bus = test_desktop::TestBus::start();
//! let desktop = test_desktop::TestDesktop::serve(bus.connect().await?).await?;
//! // … the app, on a connection of its own, posts a notification …
//! let posted = desktop.notifications.showing();
//! desktop.invoke(posted[0].id, "choice:Yes").await?;
//! // … and registers its tray item …
//! let item = desktop.tray_item().await?;
//! let (_revision, menu) = item.menu().await?;
//! item.click(menu.find("Quit").unwrap().id).await?;
//! # Ok(())
//! # }
//! ```

pub mod bus;
pub mod notifications;
pub mod secrets;
pub mod system;
pub mod tray;
pub mod watcher;

use std::collections::HashMap;

use zbus::object_server::SignalEmitter;

pub use bus::TestBus;
use notifications::Notifications;
use secrets::Secrets;
pub use system::TestSystem;
pub use tray::{MenuItem, TrayItem};
use watcher::Watcher;

const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";

/// The desktop's services on one bus connection, and what they have
/// seen.
pub struct TestDesktop {
    pub connection: zbus::Connection,
    pub notifications: Notifications,
    pub secrets: Secrets,
    pub watcher: Watcher,
}

impl TestDesktop {
    /// Serves the three services on `connection`, with GNOME Shell's
    /// notification capabilities, and takes their well-known names.
    ///
    /// # Errors
    ///
    /// Fails when the bus refuses a service's path or name.
    pub async fn serve(connection: zbus::Connection) -> zbus::Result<Self> {
        Self::serve_with(connection, &notifications::GNOME).await
    }

    /// As `serve`, with a notification server that reports
    /// `capabilities`.
    ///
    /// # Errors
    ///
    /// Fails when the bus refuses a service's path or name.
    pub async fn serve_with(
        connection: zbus::Connection,
        capabilities: &[&str],
    ) -> zbus::Result<Self> {
        let notifications = Notifications::with(capabilities);
        let secrets = Secrets::default();
        let watcher = Watcher::default();
        let server = connection.object_server();
        server.at(NOTIFICATIONS_PATH, notifications.clone()).await?;
        server
            .at(
                "/org/freedesktop/secrets",
                secrets::Service(secrets.clone()),
            )
            .await?;
        server
            .at(
                "/org/freedesktop/secrets/aliases/default",
                secrets::Collection(secrets.clone()),
            )
            .await?;
        server.at("/StatusNotifierWatcher", watcher.clone()).await?;
        server
            .at(
                CONTROL_PATH,
                Control {
                    notifications: notifications.clone(),
                },
            )
            .await?;
        for name in [
            "org.freedesktop.Notifications",
            "org.freedesktop.secrets",
            "org.kde.StatusNotifierWatcher",
            "sh.transom.TestDesktop",
        ] {
            connection.request_name(name).await?;
        }
        Ok(Self {
            connection,
            notifications,
            secrets,
            watcher,
        })
    }

    fn emitter(&self) -> zbus::Result<SignalEmitter<'_>> {
        SignalEmitter::new(&self.connection, NOTIFICATIONS_PATH)
    }

    /// Presses `action` on notification `id`, as a person would.
    ///
    /// # Errors
    ///
    /// Fails when the bus does not take the signals.
    pub async fn invoke(&self, id: u32, action: &str) -> zbus::Result<()> {
        self.notifications
            .invoke(&self.emitter()?, id, action)
            .await
    }

    /// Dismisses notification `id`, as a person would.
    ///
    /// # Errors
    ///
    /// Fails when the bus does not take the signal.
    pub async fn dismiss(&self, id: u32) -> zbus::Result<()> {
        self.notifications.dismiss(&self.emitter()?, id).await
    }

    /// The first tray item to register with the watcher, once one has,
    /// as its host sees it.
    ///
    /// # Errors
    ///
    /// Fails when the item does not answer.
    pub async fn tray_item(&self) -> zbus::Result<TrayItem> {
        TrayItem::open(&self.connection, &self.watcher.registered().await).await
    }

    /// Keeps `value` in the keyring under `attributes`, as if an earlier
    /// run of the app had saved it.
    ///
    /// # Errors
    ///
    /// Fails when the bus refuses the secret's path.
    pub async fn keep_secret(&self, attributes: &[(&str, &str)], value: &[u8]) -> zbus::Result<()> {
        let attributes: HashMap<String, String> = attributes
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        self.secrets
            .keep(self.connection.object_server(), attributes, value.to_vec())
            .await?;
        Ok(())
    }
}

const CONTROL_PATH: &str = "/sh/transom/TestDesktop";

/// What a script does to the desktop from outside the process, with
/// `gdbus` or `busctl`, where a Rust test would call `TestDesktop`.
struct Control {
    notifications: Notifications,
}

#[zbus::interface(name = "sh.transom.TestDesktop")]
impl Control {
    /// Every notification showing, as JSON.
    fn notifications(&self) -> String {
        serde_json::to_string(&self.notifications.showing()).expect("notifications are plain data")
    }

    async fn invoke(
        &self,
        id: u32,
        action: &str,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<()> {
        let emitter = SignalEmitter::new(connection, NOTIFICATIONS_PATH)?;
        Ok(self.notifications.invoke(&emitter, id, action).await?)
    }

    async fn dismiss(
        &self,
        id: u32,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> zbus::fdo::Result<()> {
        let emitter = SignalEmitter::new(connection, NOTIFICATIONS_PATH)?;
        Ok(self.notifications.dismiss(&emitter, id).await?)
    }
}
