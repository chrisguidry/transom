//! The desktop's services in-process, as any client of theirs calls
//! them: the parts of each protocol that transom's desktop app does not
//! use, and other apps do.

use futures_util::StreamExt;
use test_desktop::{TestBus, TestDesktop};
use zbus::{MatchRule, MessageStream, Proxy};

/// A desktop and a client's connection to it, on a bus of their own.
struct Session {
    desktop: TestDesktop,
    client: zbus::Connection,
    _bus: TestBus,
}

async fn session() -> Session {
    let bus = TestBus::start();
    let desktop = TestDesktop::serve(bus.connect().await.unwrap());
    Session {
        desktop: desktop.await.unwrap(),
        client: bus.connect().await.unwrap(),
        _bus: bus,
    }
}

impl Session {
    /// The client's proxy of the service `name` at `path`, whose
    /// interface has the service's name.
    async fn proxy(&self, name: &'static str, path: &'static str) -> Proxy<'static> {
        Proxy::new(&self.client, name, path, name).await.unwrap()
    }

    async fn notifications(&self) -> Proxy<'static> {
        let name = "org.freedesktop.Notifications";
        self.proxy(name, "/org/freedesktop/Notifications").await
    }

    async fn watcher(&self) -> Proxy<'static> {
        let name = "org.kde.StatusNotifierWatcher";
        self.proxy(name, "/StatusNotifierWatcher").await
    }
}

#[tokio::test]
async fn the_notification_server_names_itself() {
    let session = session().await;
    let notifications = session.notifications().await;
    let information: (String, String, String, String) = notifications
        .call("GetServerInformation", &())
        .await
        .unwrap();
    assert_eq!(information.0, "test-desktop");
    assert_eq!(information.3, "1.2");
}

#[tokio::test]
async fn closing_a_notification_that_is_not_showing_says_nothing() {
    let session = session().await;
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .member("NotificationClosed")
        .build()
        .unwrap();
    let mut closed = MessageStream::for_match_rule(rule, &session.client, None)
        .await
        .unwrap();
    let notifications = session.notifications().await;
    let (actions, hints) = (
        Vec::<&str>::new(),
        std::collections::HashMap::<&str, zbus::OwnedVariant>::new(),
    );
    let shown: u32 = notifications
        .call(
            "Notify",
            &("app", 0_u32, "", "Hello", "", actions, hints, -1),
        )
        .await
        .unwrap();
    let _: () = notifications
        .call("CloseNotification", &(shown + 1,))
        .await
        .unwrap();
    let _: () = notifications
        .call("CloseNotification", &(shown,))
        .await
        .unwrap();
    let first = closed.next().await.unwrap().unwrap();
    let (id, reason): (u32, u32) = first.body().deserialize().unwrap();
    assert_eq!((id, reason), (shown, 3));
}

#[tokio::test]
async fn an_item_that_registers_by_its_path_is_recorded_under_its_bus_name() {
    let session = session().await;
    let watcher = session.watcher().await;
    let _: () = watcher
        .call("RegisterStatusNotifierItem", &("/StatusNotifierItem",))
        .await
        .unwrap();
    let name = session.client.unique_name().unwrap();
    let registered = [format!("{name}/StatusNotifierItem")];
    assert_eq!(session.desktop.watcher.items(), registered);
    let listed: Vec<String> = watcher
        .get_property("RegisteredStatusNotifierItems")
        .await
        .unwrap();
    assert_eq!(listed, registered);
}

#[tokio::test]
async fn the_watcher_has_a_host_from_the_start() {
    let session = session().await;
    let watcher = session.watcher().await;
    let _: () = watcher
        .call(
            "RegisterStatusNotifierHost",
            &("org.kde.StatusNotifierHost-1",),
        )
        .await
        .unwrap();
    let hosted: bool = watcher
        .get_property("IsStatusNotifierHostRegistered")
        .await
        .unwrap();
    let version: i32 = watcher.get_property("ProtocolVersion").await.unwrap();
    assert_eq!((hosted, version), (true, 0));
}
