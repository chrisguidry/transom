//! `org.kde.StatusNotifierItem`, the icon itself: what a tray host reads
//! to draw it, and what the host calls when a person clicks it.

// zbus's macro reads every argument of a D-Bus method, including those
// the method ignores, in code that no attribute on the method covers.
#![allow(clippy::used_underscore_binding)]

use std::sync::{Arc, Mutex};

use zbus::ObjectPath;
use zbus::object_server::SignalEmitter;

use super::{Shown, menu};
use crate::tray::{Menu, Picked};

pub const PATH: &str = "/StatusNotifierItem";

/// The icon's name in the icon theme, drawn in the panel's own color,
/// and the one that shows while something waits.
const ICON: &str = "sh.transom-symbolic";
const ATTENTION_ICON: &str = "sh.transom-attention-symbolic";

/// An icon as pixels, each a width, a height, and ARGB bytes. The item
/// names its icons, and sends none this way.
type Pixmaps = Vec<(i32, i32, Vec<u8>)>;

/// The item's `Status`, which makes a host draw the attention icon while
/// something waits.
pub fn status(menu: &Menu) -> &'static str {
    if menu.attention() {
        "NeedsAttention"
    } else {
        "Active"
    }
}

pub struct Item {
    pub shown: Arc<Mutex<Shown>>,
    /// A directory with the app's icons, for a run that is not
    /// installed, or empty to use the system's icon theme alone.
    pub icons: String,
}

impl Item {
    fn showing(&self) -> Menu {
        let shown = self.shown.lock().expect("the lock is never poisoned");
        shown.menu.clone()
    }

    fn open_inbox(&self) {
        let mut shown = self.shown.lock().expect("the lock is never poisoned");
        shown.pick(Picked::Inbox { token: None });
    }
}

// Calls are handled in the order they arrive, so the token a host
// provides is held before the click it belongs to is handled. A property
// that changes says so with the item's own signal, which is the protocol
// hosts follow, and never with `PropertiesChanged`.
#[zbus::interface(name = "org.kde.StatusNotifierItem", spawn = false)]
// zbus calls every method of an interface on the object it serves.
#[allow(clippy::unused_self)]
impl Item {
    #[zbus(property(emits_changed_signal = "const"))]
    fn category(&self) -> &'static str {
        "Communications"
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> &'static str {
        "sh.transom"
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn title(&self) -> &'static str {
        "transom"
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn status(&self) -> &str {
        status(&self.showing())
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn window_id(&self) -> i32 {
        0
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn icon_theme_path(&self) -> &str {
        &self.icons
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn menu(&self) -> ObjectPath<'_> {
        ObjectPath::from_static_str_unchecked(menu::PATH)
    }

    /// A click on the icon opens the menu, and the item's own window
    /// only when the host activates it.
    #[zbus(property(emits_changed_signal = "const"))]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn icon_name(&self) -> &'static str {
        ICON
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn icon_pixmap(&self) -> Pixmaps {
        Pixmaps::new()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn overlay_icon_name(&self) -> &'static str {
        ""
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn overlay_icon_pixmap(&self) -> Pixmaps {
        Pixmaps::new()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn attention_icon_name(&self) -> &'static str {
        ATTENTION_ICON
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn attention_icon_pixmap(&self) -> Pixmaps {
        Pixmaps::new()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn attention_movie_name(&self) -> &'static str {
        ""
    }

    /// An icon's name, an icon's pixels, a title, and a description, of
    /// which the item gives the title.
    #[zbus(property(emits_changed_signal = "false"))]
    fn tool_tip(&self) -> (&str, Pixmaps, String, &str) {
        ("", Pixmaps::new(), self.showing().tooltip(), "")
    }

    /// The text beside the icon: how many questions wait.
    #[zbus(property(emits_changed_signal = "false"), name = "XAyatanaLabel")]
    fn label(&self) -> String {
        self.showing().count()
    }

    /// The widest text the label shows, which a host may size the label
    /// by. The count is its own guide.
    #[zbus(property(emits_changed_signal = "false"), name = "XAyatanaLabelGuide")]
    fn label_guide(&self) -> String {
        self.showing().count()
    }

    fn activate(&self, _x: i32, _y: i32) {
        self.open_inbox();
    }

    fn secondary_activate(&self, _x: i32, _y: i32) {
        self.open_inbox();
    }

    /// Holds the token of the click that the host reports next, with
    /// which a window may take the focus.
    fn provide_xdg_activation_token(&self, token: String) {
        let mut shown = self.shown.lock().expect("the lock is never poisoned");
        shown.token = Some(token);
    }

    /// The host shows the menu itself, from the item's `Menu`.
    fn context_menu(&self, _x: i32, _y: i32) {}

    fn scroll(&self, _delta: i32, _orientation: &str) {}

    #[zbus(signal)]
    pub async fn new_status(emitter: &SignalEmitter<'_>, status: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal, name = "XAyatanaNewLabel")]
    pub async fn new_label(
        emitter: &SignalEmitter<'_>,
        label: &str,
        guide: &str,
    ) -> zbus::Result<()>;
}
