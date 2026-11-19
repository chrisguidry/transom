//! The tray as a `StatusNotifierItem`, the protocol KDE Plasma hosts
//! itself and GNOME hosts through the `AppIndicator` extension Ubuntu
//! ships. The app serves two objects on the session bus: the icon, in
//! `item`, and its menu, in `menu`, which `layout` lays out. It tells
//! the session's `org.kde.StatusNotifierWatcher` about the icon, and
//! again each time a watcher appears, since a host can start after the
//! app or restart under it.

mod item;
mod layout;
mod menu;
#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;
use zbus::fdo::{DBusProxy, NameOwnerChangedStream};
use zbus::object_server::SignalEmitter;

use self::item::Item;
use self::layout::{Ids, Layout, Properties};
use self::menu::DbusMenu;
use super::{Menu, Picked, Tray};

const WATCHER: &str = "org.kde.StatusNotifierWatcher";

#[zbus::proxy(
    interface = "org.kde.StatusNotifierWatcher",
    default_service = "org.kde.StatusNotifierWatcher",
    default_path = "/StatusNotifierWatcher"
)]
trait Watcher {
    /// A session without a tray host refuses the call; the bus must not
    /// try to start one.
    #[zbus(no_autostart)]
    fn register_status_notifier_item(&self, service: &str) -> zbus::Result<()>;
}

/// What the icon and its menu show now, which both objects serve from.
struct Shown {
    menu: Menu,
    layout: Layout,
    ids: Ids,
    /// The layout's number, which goes up each time the layout changes.
    revision: u32,
    /// The token the host gave for the click it reports next.
    token: Option<String>,
    picked: UnboundedSender<Picked>,
}

/// What a host has to be told after the menu changed.
struct Changes {
    status: Option<&'static str>,
    tooltip: bool,
    count: Option<String>,
    /// The items the layout before also had, with their properties that
    /// differ now.
    properties: Vec<(i32, Properties)>,
    /// The layout's new revision.
    layout: Option<u32>,
}

impl Shown {
    fn new(picked: UnboundedSender<Picked>) -> Self {
        let menu = Menu::default();
        let mut ids = Ids::default();
        Self {
            layout: Layout::of(&menu, &mut ids),
            menu,
            ids,
            revision: 1,
            token: None,
            picked,
        }
    }

    /// Sends `picked` to the app with the token of the click, which
    /// belongs to this click alone.
    fn pick(&mut self, picked: Picked) {
        let _ = self.picked.send(picked.with(self.token.take()));
    }

    fn show(&mut self, menu: Menu) -> Changes {
        let layout = Layout::of(&menu, &mut self.ids);
        let status = item::status(&menu);
        let changes = Changes {
            status: (status != item::status(&self.menu)).then_some(status),
            tooltip: menu.tooltip() != self.menu.tooltip(),
            count: Some(menu.count()).filter(|count| *count != self.menu.count()),
            properties: layout.changed(&self.layout),
            layout: (layout != self.layout).then_some(self.revision + 1),
        };
        self.revision = changes.layout.unwrap_or(self.revision);
        self.menu = menu;
        self.layout = layout;
        changes
    }
}

/// Tells the session's tray host about the item that owns `name`. A
/// session without a host refuses, and the item waits for one.
async fn register(connection: &zbus::Connection, name: &str) {
    let registered = async {
        let watcher = WatcherProxy::new(connection).await?;
        watcher.register_status_notifier_item(name).await
    };
    if let Err(error) = registered.await {
        tracing::debug!(%error, "no tray host took the icon");
    }
}

/// Registers the item each time a watcher takes its name.
async fn follow(connection: zbus::Connection, name: String, mut owners: NameOwnerChangedStream) {
    while let Some(changed) = owners.next().await {
        if changed.args().is_ok_and(|args| args.new_owner.is_some()) {
            register(&connection, &name).await;
        }
    }
}

pub struct StatusNotifier {
    connection: zbus::Connection,
    shown: Arc<Mutex<Shown>>,
    following: JoinHandle<()>,
}

impl StatusNotifier {
    /// Serves the icon on `connection`, the session's bus, registers it
    /// with the session's tray host, and sends what people pick to
    /// `picked`. `icons` is a directory that holds the app's icons when
    /// it is not installed, or empty. A bus without a tray host still
    /// gets the icon, which registers when a host appears.
    pub async fn serve(
        connection: zbus::Connection,
        picked: UnboundedSender<Picked>,
        icons: String,
    ) -> zbus::Result<Self> {
        let shown = Arc::new(Mutex::new(Shown::new(picked)));
        let server = connection.object_server();
        let item = Item {
            shown: shown.clone(),
            icons: icons.clone(),
        };
        server.at(item::PATH, item).await?;
        let menu = DbusMenu {
            shown: shown.clone(),
            icons,
        };
        server.at(menu::PATH, menu).await?;
        // The name hosts expect of an item, which a host also watches to
        // learn that the item is gone.
        let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
        connection.request_name(name.as_str()).await?;
        // Subscribed before the first registration, so a watcher that
        // appears while it is refused is not missed.
        let owners = DBusProxy::new(&connection)
            .await?
            .receive_name_owner_changed_with_args(&[(0, WATCHER)])
            .await?;
        register(&connection, &name).await;
        Ok(Self {
            following: tokio::spawn(follow(connection.clone(), name, owners)),
            connection,
            shown,
        })
    }

    async fn tell(&self, changes: Changes) -> zbus::Result<()> {
        let item = SignalEmitter::new(&self.connection, item::PATH)?;
        if let Some(status) = changes.status {
            Item::new_status(&item, status).await?;
        }
        if changes.tooltip {
            Item::new_tool_tip(&item).await?;
        }
        if let Some(count) = &changes.count {
            Item::new_label(&item, count, count).await?;
        }
        let menu = SignalEmitter::new(&self.connection, menu::PATH)?;
        if !changes.properties.is_empty() {
            let updated = changes.properties.into_iter();
            let updated = updated
                .map(|(id, properties)| (id, menu::values(properties, &[])))
                .collect();
            DbusMenu::items_properties_updated(&menu, updated, Vec::new()).await?;
        }
        if let Some(revision) = changes.layout {
            DbusMenu::layout_updated(&menu, revision, 0).await?;
        }
        Ok(())
    }
}

impl Tray for StatusNotifier {
    async fn show(&self, menu: Menu) {
        let changes = {
            let mut shown = self.shown.lock().expect("the lock is never poisoned");
            shown.show(menu)
        };
        // The signals fail only once the session's bus is gone, and then
        // no host is left to hear them.
        let _ = self.tell(changes).await;
    }
}

impl Drop for StatusNotifier {
    fn drop(&mut self) {
        self.following.abort();
    }
}

#[cfg(test)]
mod testing {
    use crate::tray::{Menu, Question};

    /// A question from `@alice` with `choices`, each keyed by its label
    /// in lower case.
    pub fn question(exchange: &str, choices: &[&str], takes_text: bool) -> Question {
        Question {
            exchange: exchange.to_owned(),
            label: format!("@alice: Summary of {exchange}"),
            choices: choices
                .iter()
                .map(|choice| (choice.to_lowercase(), (*choice).to_owned()))
                .collect(),
            takes_text,
        }
    }

    /// The menu of `@bob`, connected, with notifications on and an empty
    /// inbox, for tests to change.
    pub fn signed_in() -> Menu {
        Menu {
            me: Some("@bob".to_owned()),
            notifications: true,
            connected: true,
            ..Menu::default()
        }
    }
}
