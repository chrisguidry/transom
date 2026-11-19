//! `org.freedesktop.secrets`, the Secret Service that GNOME Keyring
//! serves: one default collection, kept in memory, with the `plain`
//! session that carries each secret as it is. The collection is unlocked
//! until a test locks it. `Unlock` then unlocks it, or answers with a
//! prompt that no one answers, as the test chose.

// zbus's macro reads every argument of a D-Bus method, including those
// the method ignores, in code that no attribute on the method covers.
#![allow(clippy::used_underscore_binding)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::object_server::ObjectServer;
use zbus::{ObjectPath, OwnedObjectPath, OwnedVariant, Type, Variant};

const SESSION: &str = "/org/freedesktop/secrets/session/test";
const NO_PROMPT: &str = "/";
const PROMPT: &str = "/org/freedesktop/secrets/prompt/test";

/// Whether the collection gives its secrets.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Lock {
    #[default]
    Unlocked,
    /// Locked until `Unlock` is called.
    Locked,
    /// Locked, and `Unlock` answers with a prompt for the person, which
    /// no one answers, so the collection stays locked.
    Prompting,
}

/// A secret as the Secret Service passes it.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, Type)]
pub struct Secret {
    session: OwnedObjectPath,
    parameters: Vec<u8>,
    value: Vec<u8>,
    content_type: String,
}

#[derive(Clone)]
struct Kept {
    path: OwnedObjectPath,
    attributes: HashMap<String, String>,
    secret: Secret,
}

#[derive(Default)]
struct State {
    next: u32,
    items: Vec<Kept>,
    lock: Lock,
}

/// The keyring's side that a test reads and fills.
#[derive(Clone, Default)]
pub struct Secrets {
    state: Arc<Mutex<State>>,
}

fn path(text: &str) -> OwnedObjectPath {
    ObjectPath::try_from(text.to_owned())
        .expect("the path is well formed")
        .into()
}

impl Secrets {
    /// The value of each secret kept, oldest first.
    ///
    /// # Panics
    ///
    /// Panics when a test panicked while it held the lock.
    #[must_use]
    pub fn values(&self) -> Vec<Vec<u8>> {
        let state = self.state.lock().expect("no test poisons the lock");
        state
            .items
            .iter()
            .map(|item| item.secret.value.clone())
            .collect()
    }

    /// Locks the collection, as it is before the person has unlocked
    /// their keyring: a search lists each item as locked, and no item
    /// gives its secret, until `Unlock` is called.
    ///
    /// # Panics
    ///
    /// Panics when a test panicked while it held the lock.
    pub fn lock(&self) {
        self.state.lock().expect("no test poisons the lock").lock = Lock::Locked;
    }

    /// Locks the collection as `lock` does, and makes `Unlock` answer
    /// with a prompt in place of unlocking: the person has to type their
    /// password, and no one does, so the collection stays locked and
    /// takes no new item.
    ///
    /// # Panics
    ///
    /// Panics when a test panicked while it held the lock.
    pub fn lock_behind_a_prompt(&self) {
        self.state.lock().expect("no test poisons the lock").lock = Lock::Prompting;
    }

    /// Keeps `value` under `attributes`, as if an earlier run of the app
    /// had saved it, and serves it at a path of its own.
    ///
    /// # Errors
    ///
    /// Fails when the bus refuses the secret's path.
    pub async fn keep(
        &self,
        server: &ObjectServer,
        attributes: HashMap<String, String>,
        value: Vec<u8>,
    ) -> zbus::Result<OwnedObjectPath> {
        let secret = Secret {
            session: path(SESSION),
            parameters: Vec::new(),
            value,
            content_type: "application/json".to_owned(),
        };
        self.create(server, attributes, secret, true).await
    }

    async fn create(
        &self,
        server: &ObjectServer,
        attributes: HashMap<String, String>,
        secret: Secret,
        replace: bool,
    ) -> zbus::Result<OwnedObjectPath> {
        let (item, replaced) = {
            let mut state = self.state.lock().expect("no test poisons the lock");
            let same = |item: &Kept| replace && item.attributes == attributes;
            let replaced: Vec<OwnedObjectPath> = state
                .items
                .iter()
                .filter(|item| same(item))
                .map(|item| item.path.clone())
                .collect();
            state.items.retain(|item| !same(item));
            state.next += 1;
            let item = path(&format!(
                "/org/freedesktop/secrets/collection/default/{}",
                state.next
            ));
            state.items.push(Kept {
                path: item.clone(),
                attributes,
                secret,
            });
            (item, replaced)
        };
        for gone in replaced {
            server.remove::<Item, _>(&gone).await?;
        }
        server
            .at(
                &item,
                Item {
                    secrets: self.clone(),
                    path: item.clone(),
                },
            )
            .await?;
        Ok(item)
    }
}

/// The service itself, at `/org/freedesktop/secrets`.
pub(crate) struct Service(pub(crate) Secrets);

#[zbus::interface(name = "org.freedesktop.Secret.Service")]
impl Service {
    // zbus calls every method of an interface on the object it serves.
    #[allow(clippy::unused_self)]
    fn open_session(
        &self,
        _algorithm: &str,
        _input: Variant<'_>,
    ) -> (OwnedVariant, OwnedObjectPath) {
        let nothing = OwnedVariant::try_from(Variant::from("")).expect("a string is a value");
        (nothing, path(SESSION))
    }

    // zbus hands a method each argument by value.
    #[allow(clippy::needless_pass_by_value)]
    fn search_items(
        &self,
        attributes: HashMap<String, String>,
    ) -> (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) {
        let state = self.0.state.lock().expect("no test poisons the lock");
        let found = state
            .items
            .iter()
            .filter(|item| {
                attributes
                    .iter()
                    .all(|(name, value)| item.attributes.get(name) == Some(value))
            })
            .map(|item| item.path.clone())
            .collect();
        if state.lock == Lock::Unlocked {
            (found, Vec::new())
        } else {
            (Vec::new(), found)
        }
    }

    fn unlock(&self, objects: Vec<OwnedObjectPath>) -> (Vec<OwnedObjectPath>, OwnedObjectPath) {
        let mut state = self.0.state.lock().expect("no test poisons the lock");
        if state.lock == Lock::Prompting {
            return (Vec::new(), path(PROMPT));
        }
        state.lock = Lock::Unlocked;
        (objects, path(NO_PROMPT))
    }
}

/// The default collection, at `/org/freedesktop/secrets/aliases/default`.
pub(crate) struct Collection(pub(crate) Secrets);

#[zbus::interface(name = "org.freedesktop.Secret.Collection")]
impl Collection {
    async fn create_item(
        &self,
        properties: HashMap<String, OwnedVariant>,
        secret: Secret,
        replace: bool,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> zbus::fdo::Result<(OwnedObjectPath, OwnedObjectPath)> {
        let attributes: HashMap<String, String> = properties
            .get("org.freedesktop.Secret.Item.Attributes")
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| HashMap::<String, String>::try_from(value).ok())
            .unwrap_or_default();
        if self.0.state.lock().expect("no test poisons the lock").lock != Lock::Unlocked {
            return Err(zbus::fdo::Error::Failed(
                "the collection is locked".to_owned(),
            ));
        }
        let item = self
            .0
            .create(server, attributes, secret, replace)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
        Ok((item, path(NO_PROMPT)))
    }
}

/// One kept secret, at a path of its own.
struct Item {
    secrets: Secrets,
    path: OwnedObjectPath,
}

#[zbus::interface(name = "org.freedesktop.Secret.Item")]
impl Item {
    fn get_secret(&self, _session: OwnedObjectPath) -> zbus::fdo::Result<Secret> {
        let state = self.secrets.state.lock().expect("no test poisons the lock");
        state
            .items
            .iter()
            .find(|item| item.path == self.path)
            .map(|item| item.secret.clone())
            .ok_or_else(|| zbus::fdo::Error::UnknownObject("the item was deleted".to_owned()))
    }

    async fn delete(
        &self,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        self.secrets
            .state
            .lock()
            .expect("no test poisons the lock")
            .items
            .retain(|item| item.path != self.path);
        let _ = server.remove::<Item, _>(&self.path).await;
        Ok(path(NO_PROMPT))
    }
}
