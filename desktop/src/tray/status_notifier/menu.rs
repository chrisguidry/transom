//! `com.canonical.dbusmenu`, the icon's menu: a tray host reads the
//! layout to draw the menu, and reports each click by the item's id.

// zbus's macro reads every argument of a D-Bus method, including those
// the method ignores, in code that no attribute on the method covers.
#![allow(clippy::used_underscore_binding)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::object_server::SignalEmitter;
use zbus::{OwnedVariant, Variant, fdo};

use super::Shown;
use super::layout::{Item, Properties, Property};

pub const PATH: &str = "/MenuBar";

/// An item's properties as the protocol carries them.
pub type Values = HashMap<&'static str, Variant<'static>>;

/// One item of a layout: its id, its properties, and its children, each
/// in a variant.
type Node = (i32, Values, Vec<Variant<'static>>);

/// The properties among `properties` that `names` asks for, which is all
/// of them when it names none.
pub fn values(properties: Properties, names: &[String]) -> Values {
    properties
        .into_iter()
        .filter(|(name, _)| names.is_empty() || names.iter().any(|wanted| wanted == name))
        .map(|(name, property)| match property {
            Property::Text(text) => (name, Variant::from(text)),
            Property::Flag(flag) => (name, Variant::from(flag)),
            Property::Number(number) => (name, Variant::from(number)),
        })
        .collect()
}

/// `item` and its children down to `depth` levels, or all of them when
/// `depth` is negative.
fn node(item: &Item, depth: i32, names: &[String]) -> Node {
    let children = if depth == 0 {
        Vec::new()
    } else {
        let children = item.children.iter();
        children
            .map(|child| Variant::from(node(child, depth - 1, names)))
            .collect()
    };
    (item.id, values(item.properties(), names), children)
}

fn unknown(id: i32) -> fdo::Error {
    fdo::Error::InvalidArgs(format!("the menu has no item {id}"))
}

pub struct DbusMenu {
    pub shown: Arc<Mutex<Shown>>,
    /// A directory with the app's icons, or empty.
    pub icons: String,
}

// Calls are handled in the order they arrive, so the token a host
// provides is held before the click it belongs to is handled.
#[zbus::interface(name = "com.canonical.dbusmenu", spawn = false)]
// zbus calls every method of an interface on the object it serves, and
// hands a method each argument by value.
#[allow(clippy::unused_self, clippy::needless_pass_by_value)]
impl DbusMenu {
    /// The protocol's version.
    #[zbus(property(emits_changed_signal = "const"))]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn text_direction(&self) -> &'static str {
        "ltr"
    }

    /// `normal`, where `notice` would ask the host to show the menu more
    /// prominently.
    #[zbus(property(emits_changed_signal = "const"))]
    fn status(&self) -> &'static str {
        "normal"
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn icon_theme_path(&self) -> Vec<&str> {
        if self.icons.is_empty() {
            Vec::new()
        } else {
            vec![&self.icons]
        }
    }

    /// The layout's revision, and the item `parent_id` with its children
    /// down to `recursion_depth` levels.
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> fdo::Result<(u32, Node)> {
        let shown = self.shown.lock().expect("the lock is never poisoned");
        let parent = shown.layout.find(parent_id);
        let parent = parent.ok_or_else(|| unknown(parent_id))?;
        Ok((
            shown.revision,
            node(parent, recursion_depth, &property_names),
        ))
    }

    /// The properties of each item in `ids` that the menu has, or of
    /// every item when `ids` is empty.
    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        property_names: Vec<String>,
    ) -> Vec<(i32, Values)> {
        let shown = self.shown.lock().expect("the lock is never poisoned");
        let items = shown.layout.items().into_iter();
        items
            .filter(|item| ids.is_empty() || ids.contains(&item.id))
            .map(|item| (item.id, values(item.properties(), &property_names)))
            .collect()
    }

    fn get_property(&self, id: i32, name: &str) -> fdo::Result<Variant<'static>> {
        let shown = self.shown.lock().expect("the lock is never poisoned");
        let item = shown.layout.find(id).ok_or_else(|| unknown(id))?;
        values(item.properties(), &[name.to_owned()])
            .remove(name)
            .ok_or_else(|| fdo::Error::InvalidArgs(format!("item {id} has no property {name}")))
    }

    /// What a person did to item `id`. A click picks what the item
    /// means; a host also reports a menu that opened or closed, which
    /// changes nothing.
    fn event(
        &self,
        id: i32,
        event_id: &str,
        _data: Variant<'_>,
        _timestamp: u32,
    ) -> fdo::Result<()> {
        let mut shown = self.shown.lock().expect("the lock is never poisoned");
        let item = shown.layout.find(id).ok_or_else(|| unknown(id))?;
        if event_id == "clicked"
            && let Some(picked) = item.picked.clone()
        {
            shown.pick(picked);
        }
        Ok(())
    }

    /// Handles each event, and returns the ids the menu does not have.
    fn event_group(&self, events: Vec<(i32, String, OwnedVariant, u32)>) -> Vec<i32> {
        let mut unknown = Vec::new();
        for (id, event_id, data, timestamp) in events {
            if self.event(id, &event_id, data.into(), timestamp).is_err() {
                unknown.push(id);
            }
        }
        unknown
    }

    /// Whether the menu must change before the host shows item `id`,
    /// which it never must: the layout is always current.
    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    /// The ids that must change before they show, which is none, and the
    /// ids the menu does not have.
    fn about_to_show_group(&self, ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        let shown = self.shown.lock().expect("the lock is never poisoned");
        let mut unknown = ids;
        unknown.retain(|id| shown.layout.find(*id).is_none());
        (Vec::new(), unknown)
    }

    #[zbus(signal)]
    pub async fn items_properties_updated(
        emitter: &SignalEmitter<'_>,
        updated: Vec<(i32, Values)>,
        removed: Vec<(i32, Vec<String>)>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;
}
