//! The tray's menu as dbusmenu lays one out: a tree of items, each with
//! the id its host names it by, its properties, and what picking it
//! means. Nothing here speaks D-Bus.

use std::collections::HashMap;

use crate::tray::{Menu, Picked, Question};

/// What an item is in the menu, whatever it says at the moment. An
/// item's kind never changes while it keeps its key, so a host never has
/// to replace an item it already drew.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Key {
    Root,
    SignIn,
    Heading,
    /// A question, by its exchange.
    Question(String),
    /// A question's choice, by its exchange and its place among the
    /// choices.
    Choice(String, usize),
    Reply(String),
    Open(String),
    More,
    UnsentHeading,
    /// An unsent answer, by its place among them.
    Unsent(usize),
    Inbox,
    Notifications,
    Disconnected,
    Quit,
    /// The separator above the item with this key.
    Above(Box<Key>),
}

/// The ids given to a menu's items. An item keeps its id from one layout
/// to the next, so a click on an item of the layout before this one
/// still means that item, and a host that holds the menu open redraws
/// only what changed.
#[derive(Debug, Default)]
pub struct Ids {
    last: i32,
    /// The ids of the layout before this one, less those this one took.
    earlier: HashMap<Key, i32>,
    /// The ids of the layout being made.
    taken: HashMap<Key, i32>,
}

impl Ids {
    /// Forgets the ids of items that the last layout no longer had.
    fn begin(&mut self) {
        self.earlier = std::mem::take(&mut self.taken);
    }

    fn id(&mut self, key: Key) -> i32 {
        if key == Key::Root {
            return 0;
        }
        let id = self.earlier.remove(&key).unwrap_or_else(|| {
            self.last += 1;
            self.last
        });
        self.taken.insert(key, id);
        id
    }

    fn item(&mut self, key: Key, kind: Kind, label: &str) -> Item {
        Item {
            id: self.id(key),
            kind,
            label: label.to_owned(),
            picked: None,
            children: Vec::new(),
        }
    }

    /// An item that picks `picked`.
    fn picks(&mut self, key: Key, label: &str, picked: Picked) -> Item {
        Item {
            picked: Some(picked),
            ..self.item(key, Kind::Standard, label)
        }
    }

    /// An item that only says something.
    fn says(&mut self, key: Key, label: &str) -> Item {
        self.item(key, Kind::Said, label)
    }

    /// The separator above the item with the key `above`.
    fn separator(&mut self, above: Key) -> Item {
        self.item(Key::Above(Box::new(above)), Kind::Separator, "")
    }

    fn question(&mut self, question: &Question) -> Item {
        let exchange = &question.exchange;
        let mut answers = Vec::new();
        for (place, (key, label)) in question.choices.iter().enumerate() {
            let picked = Picked::Choice {
                exchange: exchange.clone(),
                key: key.clone(),
            };
            answers.push(self.picks(Key::Choice(exchange.clone(), place), label, picked));
        }
        let open = Key::Open(exchange.clone());
        let reply = Key::Reply(exchange.clone());
        if !answers.is_empty() {
            let above = if question.takes_text { &reply } else { &open };
            answers.push(self.separator(above.clone()));
        }
        if question.takes_text {
            let picked = Picked::Reply {
                exchange: exchange.clone(),
                token: None,
            };
            answers.push(self.picks(reply, "Reply…", picked));
        }
        let picked = Picked::Open {
            exchange: exchange.clone(),
            token: None,
        };
        answers.push(self.picks(open, "Open", picked));
        Item {
            children: answers,
            ..self.item(
                Key::Question(exchange.clone()),
                Kind::Submenu,
                &question.label,
            )
        }
    }

    fn items(&mut self, menu: &Menu) -> Vec<Item> {
        let inbox = Picked::Inbox { token: None };
        if menu.me.is_none() {
            return vec![
                self.picks(Key::SignIn, "Sign in…", inbox),
                self.separator(Key::Quit),
                self.picks(Key::Quit, "Quit", Picked::Quit),
            ];
        }
        let mut items = Vec::new();
        if menu.waiting.is_empty() {
            items.push(self.says(Key::Heading, "Nothing waits on you"));
        } else {
            items.push(self.says(Key::Heading, "Waiting on you"));
        }
        for question in &menu.waiting {
            items.push(self.question(question));
        }
        if menu.more > 0 {
            items.push(self.says(Key::More, &format!("and {} more", menu.more)));
        }
        if !menu.unsent.is_empty() {
            items.push(self.separator(Key::UnsentHeading));
            items.push(self.says(Key::UnsentHeading, "Not sent yet"));
            for (place, unsent) in menu.unsent.iter().enumerate() {
                items.push(self.says(Key::Unsent(place), unsent));
            }
        }
        items.push(self.separator(Key::Inbox));
        let open = match menu.unread {
            0 => "Open transom".to_owned(),
            unread => format!("Open transom · {unread} unread"),
        };
        items.push(self.picks(Key::Inbox, &open, inbox));
        items.push(Item {
            picked: Some(Picked::Notifications(!menu.notifications)),
            ..self.item(
                Key::Notifications,
                Kind::Checkmark(menu.notifications),
                "Notifications",
            )
        });
        if !menu.connected {
            items.push(self.says(Key::Disconnected, "Not connected to the server"));
        }
        items.push(self.separator(Key::Quit));
        items.push(self.picks(Key::Quit, "Quit", Picked::Quit));
        items
    }
}

/// How an item looks and behaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An item to pick.
    Standard,
    /// Words that cannot be picked.
    Said,
    Separator,
    /// An item to pick that shows a checkmark when `true`.
    Checkmark(bool),
    /// An item that opens its children as a submenu.
    Submenu,
}

/// The value of one of an item's dbusmenu properties.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Property {
    Text(String),
    Flag(bool),
    Number(i32),
}

/// An item's dbusmenu properties, by name.
pub type Properties = Vec<(&'static str, Property)>;

/// One item of the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: i32,
    pub kind: Kind,
    pub label: String,
    /// What picking the item means, without the click's token, or `None`
    /// for an item that picks nothing.
    pub picked: Option<Picked>,
    pub children: Vec<Item>,
}

impl Item {
    /// The item's properties that differ from dbusmenu's defaults: a
    /// standard item that is enabled, visible, and has no label.
    pub fn properties(&self) -> Properties {
        let text = |text: &str| Property::Text(text.to_owned());
        let label = ("label", text(&self.label));
        match self.kind {
            Kind::Standard => vec![label],
            Kind::Said => vec![label, ("enabled", Property::Flag(false))],
            Kind::Separator => vec![("type", text("separator"))],
            Kind::Checkmark(checked) => vec![
                label,
                ("toggle-type", text("checkmark")),
                ("toggle-state", Property::Number(i32::from(checked))),
            ],
            Kind::Submenu => vec![label, ("children-display", text("submenu"))],
        }
    }

    /// This item and every item under it, parents before children.
    fn all(&self) -> Vec<&Item> {
        let mut all = vec![self];
        all.extend(self.children.iter().flat_map(Item::all));
        all
    }
}

/// The whole menu, as its root item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub root: Item,
}

impl Layout {
    /// The layout of `menu`, with each item that the layout before it
    /// had under the id it had there.
    pub fn of(menu: &Menu, ids: &mut Ids) -> Self {
        ids.begin();
        Self {
            root: Item {
                children: ids.items(menu),
                ..ids.item(Key::Root, Kind::Submenu, "")
            },
        }
    }

    /// Every item, the root first and parents before children.
    pub fn items(&self) -> Vec<&Item> {
        self.root.all()
    }

    /// The item with `id`. The root's is 0.
    pub fn find(&self, id: i32) -> Option<&Item> {
        self.items().into_iter().find(|item| item.id == id)
    }

    /// Each item that `earlier` also had whose properties differ now,
    /// with the properties that differ.
    pub fn changed(&self, earlier: &Self) -> Vec<(i32, Properties)> {
        let mut changed = Vec::new();
        for item in self.items() {
            let Some(before) = earlier.find(item.id) else {
                continue;
            };
            let before = before.properties();
            let mut differ = item.properties();
            differ.retain(|property| !before.contains(property));
            if !differ.is_empty() {
                changed.push((item.id, differ));
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests;
