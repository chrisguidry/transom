//! `test-desktop` as a process on a private bus: starting, serving what a
//! script asks of it, and stopping.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, ExitStatus, Stdio};

use futures_util::StreamExt;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use serde_json::Value;
use test_desktop::TestBus;
use zbus::fdo::DBusProxy;
use zbus::{MatchRule, MessageStream, OwnedObjectPath, OwnedVariant, Proxy};

/// A running `test-desktop`, the bus it serves on, and a connection of
/// the test's own to that bus.
struct Desktop {
    child: Child,
    connection: zbus::Connection,
    _bus: TestBus,
}

/// Starts `test-desktop` with `flags` on a bus of its own, and returns
/// once it has printed `ready`.
async fn start(flags: &[&str]) -> Desktop {
    let bus = TestBus::start();
    let mut child = Command::new(env!("CARGO_BIN_EXE_test-desktop"))
        .args(flags)
        .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready, "ready\n");
    Desktop {
        child,
        connection: bus.connect().await.unwrap(),
        _bus: bus,
    }
}

impl Desktop {
    /// The proxy of `interface` at `path` of the service `name`.
    async fn proxy(&self, name: &str, path: &str, interface: &str) -> Proxy<'static> {
        let (name, path) = (name.to_owned(), path.to_owned());
        let proxy = Proxy::new(&self.connection, name, path, interface.to_owned());
        proxy.await.unwrap()
    }

    /// The process's own interface, which a script calls.
    async fn control(&self) -> Proxy<'static> {
        let (name, path) = ("sh.transom.TestDesktop", "/sh/transom/TestDesktop");
        self.proxy(name, path, name).await
    }

    /// The notification server.
    async fn notifications(&self) -> Proxy<'static> {
        let name = "org.freedesktop.Notifications";
        self.proxy(name, "/org/freedesktop/Notifications", name)
            .await
    }

    /// Posts a notification as an app would, and returns its id.
    async fn notify(&self, summary: &str) -> u32 {
        let (actions, hints) = (
            vec!["default", "Open"],
            HashMap::<&str, OwnedVariant>::new(),
        );
        let body = ("app", 0_u32, "", summary, "", actions, hints, -1);
        let notifications = self.notifications().await;
        notifications.call("Notify", &body).await.unwrap()
    }

    /// The notifications showing, as the process lists them to a script.
    async fn listed(&self) -> Value {
        let listed: String = self
            .control()
            .await
            .call("Notifications", &())
            .await
            .unwrap();
        serde_json::from_str(&listed).unwrap()
    }

    /// Each signal the notification server sends from now on.
    async fn signals(&self) -> MessageStream {
        let rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.Notifications")
            .build()
            .unwrap();
        let signals = MessageStream::for_match_rule(rule, &self.connection, None);
        signals.await.unwrap()
    }

    /// Sends the process `signal` and waits for it to exit.
    fn stop(&mut self, signal: Signal) -> ExitStatus {
        kill(Pid::from_raw(self.child.id().try_into().unwrap()), signal).unwrap();
        self.child.wait().unwrap()
    }
}

impl Drop for Desktop {
    /// Asks the process to stop and waits until it has, so no test
    /// leaves one behind.
    fn drop(&mut self) {
        let _ = kill(
            Pid::from_raw(self.child.id().try_into().unwrap()),
            Signal::SIGTERM,
        );
        let _ = self.child.wait();
    }
}

/// The name of the next signal in `signals`.
async fn next(signals: &mut MessageStream) -> String {
    let signal = signals.next().await.unwrap().unwrap();
    signal.header().member().unwrap().to_string()
}

#[tokio::test]
async fn it_is_ready_once_each_service_has_its_name() {
    let desktop = start(&[]).await;
    let bus = DBusProxy::new(&desktop.connection).await.unwrap();
    for name in [
        "org.freedesktop.Notifications",
        "org.freedesktop.secrets",
        "org.kde.StatusNotifierWatcher",
        "sh.transom.TestDesktop",
    ] {
        let owned = bus.name_has_owner(name.try_into().unwrap()).await;
        assert!(owned.unwrap(), "{name}");
    }
}

#[tokio::test]
async fn it_keeps_the_secret_it_is_started_with_as_the_apps() {
    let desktop = start(&["--secret", "a session"]).await;
    let (name, path) = ("org.freedesktop.secrets", "/org/freedesktop/secrets");
    let service = desktop.proxy(name, path, "org.freedesktop.Secret.Service");
    let ours = HashMap::from([("application", "sh.transom")]);
    let (unlocked, _locked): (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) =
        service.await.call("SearchItems", &(ours,)).await.unwrap();
    let item = desktop.proxy(name, unlocked[0].as_str(), "org.freedesktop.Secret.Item");
    let session = OwnedObjectPath::try_from("/").unwrap();
    let (_, _, value, _): (OwnedObjectPath, Vec<u8>, Vec<u8>, String) =
        item.await.call("GetSecret", &(session,)).await.unwrap();
    assert_eq!(value, b"a session");
}

#[tokio::test]
async fn it_lists_the_notifications_showing_as_json() {
    let desktop = start(&[]).await;
    let id = desktop.notify("Restart postgres?").await;
    let listed = desktop.listed().await;
    assert_eq!(listed[0]["id"], id);
    assert_eq!(listed[0]["summary"], "Restart postgres?");
    assert_eq!(listed[0]["actions"][0][0], "default");
}

#[tokio::test]
async fn it_presses_a_notification_for_a_script() {
    let desktop = start(&[]).await;
    let id = desktop.notify("Restart postgres?").await;
    let mut signals = desktop.signals().await;
    let control = desktop.control().await;
    let _: () = control.call("Invoke", &(id, "default")).await.unwrap();
    let sent = [
        next(&mut signals).await,
        next(&mut signals).await,
        next(&mut signals).await,
    ];
    assert_eq!(
        sent,
        ["ActivationToken", "ActionInvoked", "NotificationClosed"]
    );
    assert_eq!(desktop.listed().await, Value::Array(Vec::new()));
}

#[tokio::test]
async fn it_dismisses_a_notification_for_a_script() {
    let desktop = start(&[]).await;
    let id = desktop.notify("Restart postgres?").await;
    let mut signals = desktop.signals().await;
    let control = desktop.control().await;
    let _: () = control.call("Dismiss", &(id,)).await.unwrap();
    assert_eq!(next(&mut signals).await, "NotificationClosed");
    assert_eq!(desktop.listed().await, Value::Array(Vec::new()));
}

#[tokio::test]
async fn it_stops_at_sigterm() {
    let mut desktop = start(&[]).await;
    assert!(desktop.stop(Signal::SIGTERM).success());
}

#[tokio::test]
async fn it_stops_at_sigint() {
    let mut desktop = start(&[]).await;
    assert!(desktop.stop(Signal::SIGINT).success());
}

#[tokio::test]
async fn it_fails_without_a_bus() {
    let bus = TestBus::start();
    let address = bus.address().to_owned();
    drop(bus);
    let run = Command::new(env!("CARGO_BIN_EXE_test-desktop"))
        .env("DBUS_SESSION_BUS_ADDRESS", address)
        .stderr(Stdio::null())
        .status();
    assert!(!run.unwrap().success());
}
