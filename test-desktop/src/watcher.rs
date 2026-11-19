//! `org.kde.StatusNotifierWatcher`, the tray's registry, as a desktop
//! with a tray host serves it. It records each item that registers; a
//! test reads an item's icon and menu from the item itself, as
//! `TrayItem`.

// zbus's macro reads every argument of a D-Bus method, including those
// the method ignores, in code that no attribute on the method covers.
#![allow(clippy::used_underscore_binding)]

use tokio::sync::watch;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;

/// The watcher's side that a test reads.
#[derive(Clone, Default)]
pub struct Watcher {
    items: watch::Sender<Vec<String>>,
}

impl Watcher {
    /// Each registered item, as `{bus name}{object path}`.
    #[must_use]
    pub fn items(&self) -> Vec<String> {
        self.items.borrow().clone()
    }

    /// The first item to register, once one has.
    ///
    /// # Panics
    ///
    /// Never: the watcher holds the sender this waits on.
    pub async fn registered(&self) -> String {
        let mut items = self.items.subscribe();
        let items = items
            .wait_for(|items| !items.is_empty())
            .await
            .expect("the watcher holds the sender");
        items[0].clone()
    }
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    /// An item registers by its object path, or by its bus name when it
    /// serves the item at `/StatusNotifierItem`.
    async fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string).unwrap_or_default();
        let item = if service.starts_with('/') {
            format!("{sender}{service}")
        } else {
            format!("{service}/StatusNotifierItem")
        };
        self.items.send_modify(|items| items.push(item.clone()));
        Self::status_notifier_item_registered(&emitter, &item).await?;
        Ok(())
    }

    // zbus calls every method of an interface on the object it serves.
    #[allow(clippy::unused_self)]
    fn register_status_notifier_host(&self, _service: &str) {}

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items()
    }

    #[zbus(property)]
    // zbus calls every method of an interface on the object it serves.
    #[allow(clippy::unused_self)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    // zbus calls every method of an interface on the object it serves.
    #[allow(clippy::unused_self)]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}
