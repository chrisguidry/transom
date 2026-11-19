//! Where the session is kept between runs: the desktop's Secret Service,
//! which GNOME Keyring and `KWallet` both serve, in the person's default
//! collection. The session's token is the only secret the app holds.

use std::collections::HashMap;

use zbus::{ObjectPath, OwnedObjectPath, OwnedVariant, Proxy, Type, Variant};

use super::Session;

/// What tells the app's item from every other in the collection.
const APPLICATION: &str = "sh.transom";

const SERVICE: &str = "org.freedesktop.secrets";
const ITEM: &str = "org.freedesktop.Secret.Item";

#[zbus::proxy(
    interface = "org.freedesktop.Secret.Service",
    default_service = "org.freedesktop.secrets",
    default_path = "/org/freedesktop/secrets"
)]
trait Service {
    fn open_session(
        &self,
        algorithm: &str,
        input: &Variant<'_>,
    ) -> zbus::Result<(OwnedVariant, OwnedObjectPath)>;

    fn search_items(
        &self,
        attributes: HashMap<&str, &str>,
    ) -> zbus::Result<(Vec<OwnedObjectPath>, Vec<OwnedObjectPath>)>;

    fn unlock(
        &self,
        objects: &[ObjectPath<'_>],
    ) -> zbus::Result<(Vec<OwnedObjectPath>, OwnedObjectPath)>;
}

#[zbus::proxy(
    interface = "org.freedesktop.Secret.Collection",
    default_service = "org.freedesktop.secrets",
    default_path = "/org/freedesktop/secrets/aliases/default"
)]
trait Collection {
    fn create_item(
        &self,
        properties: HashMap<&str, Variant<'_>>,
        secret: Secret,
        replace: bool,
    ) -> zbus::Result<(OwnedObjectPath, OwnedObjectPath)>;
}

/// A secret as the Secret Service passes it: the session it is encoded
/// for, the encoding's parameters, the value, and its content type.
#[derive(Debug, serde::Deserialize, serde::Serialize, Type)]
struct Secret {
    session: OwnedObjectPath,
    parameters: Vec<u8>,
    value: Vec<u8>,
    content_type: String,
}

/// The app's item in the default collection.
pub struct Keyring {
    connection: zbus::Connection,
}

impl Keyring {
    /// The keyring on `connection`, the session's bus.
    pub fn new(connection: zbus::Connection) -> Self {
        Self { connection }
    }

    /// The app's items, unlocked. The message bus carries each secret
    /// unencrypted between two processes of one person's session, which
    /// the `plain` algorithm says.
    async fn items(&self) -> zbus::Result<(OwnedObjectPath, Vec<OwnedObjectPath>)> {
        let service = ServiceProxy::new(&self.connection).await?;
        let (_, session) = service.open_session("plain", &Variant::from("")).await?;
        let (mut unlocked, locked) = service
            .search_items(HashMap::from([("application", APPLICATION)]))
            .await?;
        if !locked.is_empty() {
            let locked: Vec<ObjectPath<'_>> = locked.iter().map(|path| path.as_ref()).collect();
            let (now_unlocked, _prompt) = service.unlock(&locked).await?;
            unlocked.extend(now_unlocked);
        }
        Ok((session, unlocked))
    }

    /// One item of the collection, which has a path of its own.
    async fn item(&self, path: OwnedObjectPath) -> zbus::Result<Proxy<'static>> {
        Proxy::new(&self.connection, SERVICE, path, ITEM).await
    }

    /// The session kept from an earlier run, if there is one.
    pub async fn load(&self) -> Option<Session> {
        let (session, items) = self.items().await.ok()?;
        let item = self.item(items.first()?.clone()).await.ok()?;
        let secret: Secret = item.call("GetSecret", &(session,)).await.ok()?;
        serde_json::from_slice(&secret.value).ok()
    }

    /// Keeps `kept` in place of whatever was kept before.
    pub async fn save(&self, kept: &Session) -> zbus::Result<()> {
        let service = ServiceProxy::new(&self.connection).await?;
        let (_, session) = service.open_session("plain", &Variant::from("")).await?;
        let collection = CollectionProxy::new(&self.connection).await?;
        let default = ObjectPath::try_from("/org/freedesktop/secrets/aliases/default")?;
        service.unlock(&[default]).await?;
        let label = format!("transom session for {}", kept.me);
        let properties = HashMap::from([
            (
                "org.freedesktop.Secret.Item.Label",
                Variant::from(label.as_str()),
            ),
            (
                "org.freedesktop.Secret.Item.Attributes",
                Variant::from(HashMap::from([("application", APPLICATION)])),
            ),
        ]);
        let secret = Secret {
            session,
            parameters: Vec::new(),
            value: serde_json::to_vec(kept).expect("a session is plain data"),
            content_type: "application/json".to_owned(),
        };
        collection.create_item(properties, secret, true).await?;
        Ok(())
    }

    /// Forgets the session, as signing out does.
    pub async fn clear(&self) {
        let Ok((_, items)) = self.items().await else {
            return;
        };
        for path in items {
            let deleted = async {
                let item = self.item(path).await?;
                item.call::<_, _, OwnedObjectPath>("Delete", &()).await
            };
            // An item that cannot be deleted stays until the next
            // sign-in, whose session takes its place.
            let _ = deleted.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use test_desktop::{TestBus, TestDesktop};

    use super::*;

    /// The app's keyring and the desktop that serves it, on a bus of
    /// their own.
    struct Kept {
        keyring: Keyring,
        desktop: TestDesktop,
        _bus: TestBus,
    }

    async fn kept() -> Kept {
        let bus = TestBus::start();
        let desktop = TestDesktop::serve(bus.connect().await.unwrap());
        Kept {
            desktop: desktop.await.unwrap(),
            keyring: Keyring::new(bus.connect().await.unwrap()),
            _bus: bus,
        }
    }

    fn session(me: &str) -> Session {
        Session {
            server: "https://transom.example.com/".parse().unwrap(),
            token: format!("token of {me}"),
            me: me.to_owned(),
        }
    }

    const OURS: [(&str, &str); 1] = [("application", "sh.transom")];

    #[tokio::test]
    async fn an_empty_keyring_has_no_session() {
        assert_eq!(kept().await.keyring.load().await, None);
    }

    #[tokio::test]
    async fn a_saved_session_is_loaded() {
        let kept = kept().await;
        kept.keyring.save(&session("@bob")).await.unwrap();
        assert_eq!(kept.keyring.load().await, Some(session("@bob")));
    }

    #[tokio::test]
    async fn a_session_an_earlier_run_kept_is_loaded() {
        let kept = kept().await;
        let earlier = serde_json::to_vec(&session("@bob")).unwrap();
        kept.desktop.keep_secret(&OURS, &earlier).await.unwrap();
        assert_eq!(kept.keyring.load().await, Some(session("@bob")));
    }

    #[tokio::test]
    async fn a_secret_that_is_no_session_is_not_loaded() {
        let kept = kept().await;
        kept.desktop.keep_secret(&OURS, b"not json").await.unwrap();
        assert_eq!(kept.keyring.load().await, None);
    }

    #[tokio::test]
    async fn another_apps_secret_is_not_loaded() {
        let kept = kept().await;
        let theirs = serde_json::to_vec(&session("@eve")).unwrap();
        let attributes = [("application", "sh.other")];
        kept.desktop
            .keep_secret(&attributes, &theirs)
            .await
            .unwrap();
        assert_eq!(kept.keyring.load().await, None);
    }

    #[tokio::test]
    async fn a_saved_session_takes_the_place_of_the_one_before() {
        let kept = kept().await;
        kept.keyring.save(&session("@bob")).await.unwrap();
        kept.keyring.save(&session("@robert")).await.unwrap();
        assert_eq!(kept.keyring.load().await, Some(session("@robert")));
        assert_eq!(kept.desktop.secrets.values().len(), 1);
    }

    #[tokio::test]
    async fn a_locked_keyring_is_unlocked_to_load_the_session() {
        let kept = kept().await;
        kept.keyring.save(&session("@bob")).await.unwrap();
        kept.desktop.secrets.lock();
        assert_eq!(kept.keyring.load().await, Some(session("@bob")));
    }

    #[tokio::test]
    async fn a_keyring_the_person_has_to_unlock_gives_and_takes_no_session() {
        let kept = kept().await;
        kept.keyring.save(&session("@bob")).await.unwrap();
        kept.desktop.secrets.lock_behind_a_prompt();
        assert_eq!(kept.keyring.load().await, None);
        assert!(kept.keyring.save(&session("@robert")).await.is_err());
    }

    #[tokio::test]
    async fn a_cleared_keyring_has_no_session() {
        let kept = kept().await;
        kept.keyring.save(&session("@bob")).await.unwrap();
        kept.keyring.clear().await;
        assert_eq!(kept.keyring.load().await, None);
        assert_eq!(kept.desktop.secrets.values(), Vec::<Vec<u8>>::new());
    }

    #[tokio::test]
    async fn a_desktop_without_a_keyring_keeps_nothing() {
        let bus = TestBus::start();
        let keyring = Keyring::new(bus.connect().await.unwrap());
        assert!(keyring.save(&session("@bob")).await.is_err());
        keyring.clear().await;
        assert_eq!(keyring.load().await, None);
    }
}
