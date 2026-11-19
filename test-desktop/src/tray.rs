//! A tray item as its host sees it: the calls a host such as Ubuntu's
//! `AppIndicator` extension makes to an `org.kde.StatusNotifierItem` and
//! to the `com.canonical.dbusmenu` it names, so a test reads the icon's
//! properties and its menu, and clicks as a person would.

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use zbus::proxy::{self, CacheProperties};
use zbus::{MatchRule, MessageStream, OwnedObjectPath, OwnedVariant, Proxy, Type};

const ITEM: &str = "org.kde.StatusNotifierItem";
const MENU: &str = "com.canonical.dbusmenu";

/// One menu item as dbusmenu's `GetLayout` gives it: its id, its
/// properties, and its children, each in a variant.
type Node = (i32, HashMap<String, OwnedVariant>, Vec<OwnedVariant>);

/// One item of a tray item's menu, with the protocol's defaults for the
/// properties the item left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    /// Whether a checkmark item is checked, or `None` for any other.
    pub checked: Option<bool>,
    /// Whether the item's children open as a submenu.
    pub submenu: bool,
    pub children: Vec<MenuItem>,
}

impl MenuItem {
    fn read((id, properties, children): Node) -> zbus::Result<Self> {
        let text = |name: &str| {
            properties
                .get(name)
                .and_then(|value| value.downcast_ref().ok())
        };
        let label: Option<&str> = text("label");
        let kind: Option<&str> = text("type");
        let display: Option<&str> = text("children-display");
        let toggles: Option<&str> = text("toggle-type");
        let enabled = properties.get("enabled").map(bool::try_from);
        let state = properties.get("toggle-state").map(i32::try_from);
        Ok(Self {
            id,
            label: label.unwrap_or_default().to_owned(),
            enabled: enabled.transpose()?.unwrap_or(true),
            separator: kind == Some("separator"),
            checked: toggles
                .filter(|toggles| *toggles == "checkmark")
                .and(state.transpose()?)
                .map(|state| state == 1),
            submenu: display == Some("submenu"),
            children: children
                .into_iter()
                .map(|child| Self::read(Node::try_from(child)?))
                .collect::<zbus::Result<_>>()?,
        })
    }

    /// The first item under this one, at any depth, labeled `label`.
    #[must_use]
    pub fn find(&self, label: &str) -> Option<&Self> {
        self.children.iter().find_map(|child| {
            if child.label == label {
                Some(child)
            } else {
                child.find(label)
            }
        })
    }

    /// The menu under this item as a person reads it, a line for each
    /// item: a separator as `---`, an item that cannot be picked in
    /// parentheses, a checkmark item after `[x]` or `[ ]`, a submenu
    /// before `>`, and each submenu's items indented under it.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for child in &self.children {
            lines.push(child.line());
            lines.extend(child.lines().into_iter().map(|line| format!("  {line}")));
        }
        lines
    }

    fn line(&self) -> String {
        match (self.separator, self.enabled, self.checked, self.submenu) {
            (true, ..) => "---".to_owned(),
            (_, false, ..) => format!("({})", self.label),
            (_, _, Some(true), _) => format!("[x] {}", self.label),
            (_, _, Some(false), _) => format!("[ ] {}", self.label),
            (_, _, _, true) => format!("{} >", self.label),
            _ => self.label.clone(),
        }
    }
}

/// A registered tray item, and the bus connection its host calls it on.
pub struct TrayItem {
    item: Proxy<'static>,
    menu: Proxy<'static>,
}

impl TrayItem {
    /// The item a watcher recorded as `registered`, which is
    /// `{bus name}{object path}`, with the menu the item names.
    ///
    /// # Errors
    ///
    /// Fails when the item does not answer for its `Menu` property.
    pub async fn open(connection: &zbus::Connection, registered: &str) -> zbus::Result<Self> {
        let path = registered.find('/').unwrap_or(registered.len());
        let (name, path) = registered.split_at(path);
        // Neither proxy keeps the properties it has read: the protocol
        // has signals of its own for a change, so a host asks again.
        let proxy = || {
            proxy::Builder::new(connection)
                .destination(name.to_owned())
                .cache_properties(CacheProperties::No)
        };
        let item: Proxy<'static> = proxy()
            .path(path.to_owned())
            .interface(ITEM)
            .build()
            .await?;
        let menu: OwnedObjectPath = item.get_property("Menu").await?;
        let menu = proxy().path(menu).interface(MENU).build().await?;
        Ok(Self { item, menu })
    }

    /// The item's property `name`, asked of the item each time.
    ///
    /// # Errors
    ///
    /// Fails when the item has no such property, or one of another type.
    pub async fn property<T: DeserializeOwned + Type>(&self, name: &str) -> zbus::Result<T> {
        self.item.get_property(name).await
    }

    /// Every property of the item, as a host reads them when the item
    /// registers.
    ///
    /// # Errors
    ///
    /// Fails when the item refuses the call.
    pub async fn properties(&self) -> zbus::Result<HashMap<String, OwnedVariant>> {
        let (item, properties) = (&self.item, "org.freedesktop.DBus.Properties");
        let reply = item
            .connection()
            .call_method(
                Some(item.destination()),
                item.path(),
                Some(properties),
                "GetAll",
                &(ITEM,),
            )
            .await?;
        reply.body().deserialize()
    }

    /// The menu's property `name`, asked of the menu each time.
    ///
    /// # Errors
    ///
    /// Fails when the menu has no such property, or one of another type.
    pub async fn menu_property<T: DeserializeOwned + Type>(&self, name: &str) -> zbus::Result<T> {
        self.menu.get_property(name).await
    }

    /// Gives the item the token of the click that follows, as the host
    /// does before `activate` and before each `click`.
    ///
    /// # Errors
    ///
    /// Fails when the item refuses the call.
    pub async fn provide_activation_token(&self, token: &str) -> zbus::Result<()> {
        self.item.call("ProvideXdgActivationToken", &(token,)).await
    }

    /// Activates the item, as a double click on the icon does.
    ///
    /// # Errors
    ///
    /// Fails when the item refuses the call.
    pub async fn activate(&self) -> zbus::Result<()> {
        self.item.call("Activate", &(0, 0)).await
    }

    /// Activates the item the second way, as a middle click on the icon
    /// does.
    ///
    /// # Errors
    ///
    /// Fails when the item refuses the call.
    pub async fn secondary_activate(&self) -> zbus::Result<()> {
        self.item.call("SecondaryActivate", &(0, 0)).await
    }

    /// The whole menu, as its root item, and the layout's revision.
    ///
    /// # Errors
    ///
    /// Fails when the menu refuses the call or sends a malformed layout.
    pub async fn menu(&self) -> zbus::Result<(u32, MenuItem)> {
        self.layout(0, -1, &[]).await
    }

    /// The item `parent` with its children down to `depth` levels, or
    /// all of them when `depth` is negative, each with the properties
    /// `names` lists, or every property when it lists none; and the
    /// layout's revision.
    ///
    /// # Errors
    ///
    /// Fails when the menu has no such item or sends a malformed layout.
    pub async fn layout(
        &self,
        parent: i32,
        depth: i32,
        names: &[&str],
    ) -> zbus::Result<(u32, MenuItem)> {
        let (revision, root): (u32, Node) =
            self.menu.call("GetLayout", &(parent, depth, names)).await?;
        Ok((revision, MenuItem::read(root)?))
    }

    /// Clicks the menu item `id`.
    ///
    /// # Errors
    ///
    /// Fails when the menu has no such item.
    pub async fn click(&self, id: i32) -> zbus::Result<()> {
        let data = zbus::Variant::from(0);
        self.menu.call("Event", &(id, "clicked", data, 0_u32)).await
    }

    /// The item's proxy and its menu's, for the calls a host makes that
    /// no helper here covers.
    #[must_use]
    pub fn proxies(&self) -> (&Proxy<'static>, &Proxy<'static>) {
        (&self.item, &self.menu)
    }

    /// Every signal the item and its menu send from now on, in the order
    /// sent.
    ///
    /// # Errors
    ///
    /// Fails when the bus refuses the subscription.
    pub async fn signals(&self) -> zbus::Result<MessageStream> {
        let rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(self.item.destination().to_owned())
            .build()?;
        MessageStream::for_match_rule(rule, self.item.connection(), None).await
    }
}
