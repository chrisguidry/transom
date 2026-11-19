//! The tray on a private bus, with `test-desktop` as its host: each test
//! makes the calls a host makes and reads what the app was told. The
//! tests of the icon are here, and those of its menu in `menu`.

mod menu;

use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use futures_util::StreamExt;
use test_desktop::{MenuItem, TestBus, TestDesktop, TrayItem};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use zbus::{MessageStream, OwnedVariant};

use super::testing::{question, signed_in};
use super::*;

/// The item's bus name, and what a watcher records it as.
fn name() -> String {
    format!("org.kde.StatusNotifierItem-{}-1", std::process::id())
}

/// The app's tray on `bus`, with `icons` as its icon directory, and what
/// people pick from it.
async fn tray(bus: &TestBus, icons: &str) -> (StatusNotifier, UnboundedReceiver<Picked>) {
    let (sender, picked) = unbounded_channel();
    let connection = bus.connect().await.unwrap();
    let tray = StatusNotifier::serve(connection, sender, icons.to_owned());
    (tray.await.unwrap(), picked)
}

/// A desktop with a tray host on `bus`.
async fn desktop(bus: &TestBus) -> TestDesktop {
    let connection = bus.connect().await.unwrap();
    TestDesktop::serve(connection).await.unwrap()
}

/// A desktop with a tray host, the app's tray registered with it, and
/// the tray as the host sees it, all on a bus of their own.
struct Session {
    host: TrayItem,
    tray: StatusNotifier,
    picked: UnboundedReceiver<Picked>,
    desktop: TestDesktop,
    bus: TestBus,
}

async fn session() -> Session {
    let bus = TestBus::start();
    let desktop = desktop(&bus).await;
    let (tray, picked) = tray(&bus, "/icons").await;
    Session {
        host: desktop.tray_item().await.unwrap(),
        tray,
        picked,
        desktop,
        bus,
    }
}

/// A session whose tray shows `menu`.
async fn showing(menu: Menu) -> Session {
    let session = session().await;
    session.tray.show(menu).await;
    session
}

#[tokio::test]
async fn the_item_registers_with_the_watcher() {
    let session = session().await;
    assert_eq!(
        session.desktop.watcher.items(),
        [format!("{}/StatusNotifierItem", name())]
    );
}

/// `waits`, which fails the test when it takes longer than a test's
/// D-Bus calls ever do.
async fn soon<T>(waits: impl Future<Output = T>) -> T {
    let waits = tokio::time::timeout(Duration::from_secs(10), waits);
    waits.await.expect("the bus answers within ten seconds")
}

#[tokio::test]
async fn an_item_started_before_the_watcher_registers_when_it_appears() {
    let bus = TestBus::start();
    let (_tray, _picked) = tray(&bus, "").await;
    let desktop = desktop(&bus).await;
    let item = soon(desktop.watcher.registered()).await;
    assert_eq!(item, format!("{}/StatusNotifierItem", name()));
}

#[tokio::test]
async fn the_item_registers_again_with_a_watcher_that_replaces_the_first() {
    let Session {
        host,
        desktop: first,
        tray: _tray,
        bus,
        ..
    } = session().await;
    // The first desktop's connection closes with its last holder, and
    // the bus then takes the watcher's name from it.
    drop((host, first));
    let second = desktop(&bus).await;
    let item = soon(second.watcher.registered()).await;
    assert_eq!(item, format!("{}/StatusNotifierItem", name()));
}

#[tokio::test]
async fn the_item_stops_waiting_for_watchers_when_the_bus_is_gone() {
    let mut session = session().await;
    drop(session.bus);
    let stopped = soon(&mut session.tray.following).await;
    assert!(stopped.is_ok());
}

#[tokio::test]
async fn a_desktop_without_a_tray_takes_the_menu_and_shows_nothing() {
    let tray: Option<StatusNotifier> = None;
    tray.show(signed_in()).await;
}

#[tokio::test]
async fn a_tray_the_desktop_has_shows_the_menu() {
    let session = session().await;
    let tray = Some(session.tray);
    tray.show(signed_in()).await;
    assert_eq!(tooltip(&session.host).await, "transom: @bob");
}

#[tokio::test]
async fn the_item_has_every_property_a_host_reads() {
    let session = session().await;
    let properties = session.host.properties().await.unwrap();
    let mut names: Vec<&str> = properties.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "AttentionIconName",
            "AttentionIconPixmap",
            "AttentionMovieName",
            "Category",
            "IconName",
            "IconPixmap",
            "IconThemePath",
            "Id",
            "ItemIsMenu",
            "Menu",
            "OverlayIconName",
            "OverlayIconPixmap",
            "Status",
            "Title",
            "ToolTip",
            "WindowId",
            "XAyatanaLabel",
            "XAyatanaLabelGuide",
        ]
    );
}

/// The signatures of the item's properties whose types are not plain
/// strings, which a host checks before it uses them.
#[tokio::test]
async fn the_items_properties_have_the_types_the_protocol_gives_them() {
    let session = session().await;
    let properties = session.host.properties().await.unwrap();
    let signature = |name: &str| properties[name].value_signature().to_string();
    assert_eq!(signature("ToolTip"), "(sa(iiay)ss)");
    assert_eq!(signature("IconPixmap"), "a(iiay)");
    assert_eq!(signature("OverlayIconPixmap"), "a(iiay)");
    assert_eq!(signature("AttentionIconPixmap"), "a(iiay)");
    assert_eq!(signature("Menu"), "o");
    assert_eq!(signature("ItemIsMenu"), "b");
    assert_eq!(signature("WindowId"), "i");
}

async fn text(host: &TrayItem, name: &str) -> String {
    host.property(name).await.unwrap()
}

#[tokio::test]
async fn the_item_says_what_it_is() {
    let session = session().await;
    let host = &session.host;
    assert_eq!(text(host, "Id").await, "sh.transom");
    assert_eq!(text(host, "Title").await, "transom");
    assert_eq!(text(host, "Category").await, "Communications");
    assert_eq!(text(host, "IconName").await, "sh.transom-symbolic");
    assert_eq!(
        text(host, "AttentionIconName").await,
        "sh.transom-attention-symbolic"
    );
    assert_eq!(text(host, "IconThemePath").await, "/icons");
    assert!(!host.property::<bool>("ItemIsMenu").await.unwrap());
}

/// The tooltip's title, which is all of the tooltip the item fills in.
async fn tooltip(host: &TrayItem) -> String {
    type ToolTip = (String, Vec<(i32, i32, Vec<u8>)>, String, String);
    let (_, _, title, _): ToolTip = host.property("ToolTip").await.unwrap();
    title
}

#[tokio::test]
async fn a_signed_out_item_is_active_with_no_count() {
    let session = session().await;
    assert_eq!(text(&session.host, "Status").await, "Active");
    assert_eq!(text(&session.host, "XAyatanaLabel").await, "");
    assert_eq!(text(&session.host, "XAyatanaLabelGuide").await, "");
    assert_eq!(tooltip(&session.host).await, "transom: signed out");
}

/// `@bob`'s menu with two questions listed, one more that is not, and
/// one answer not sent yet.
fn asked() -> Menu {
    Menu {
        waiting: vec![
            question("a", &["Yes", "No"], false),
            question("b", &["Now"], true),
        ],
        more: 1,
        unsent: vec!["Summary of c: Your answer, No, is not sent yet".to_owned()],
        ..signed_in()
    }
}

#[tokio::test]
async fn an_item_with_questions_asks_for_attention_and_counts_them() {
    let session = showing(asked()).await;
    assert_eq!(text(&session.host, "Status").await, "NeedsAttention");
    assert_eq!(text(&session.host, "XAyatanaLabel").await, "3");
    assert_eq!(text(&session.host, "XAyatanaLabelGuide").await, "3");
    assert_eq!(
        tooltip(&session.host).await,
        "transom: 3 questions wait on @bob"
    );
}

/// The id of the first item of `menu` labeled `label`.
fn id(menu: &MenuItem, label: &str) -> i32 {
    menu.find(label).unwrap().id
}

/// The next signal's name and its arguments.
async fn heard(signals: &mut MessageStream) -> (String, zbus::Message) {
    let signal = soon(signals.next()).await.unwrap().unwrap();
    let member = signal.header().member().unwrap().to_string();
    (member, signal)
}

#[tokio::test]
async fn a_changed_menu_is_announced_to_the_host() {
    let session = session().await;
    let mut signals = session.host.signals().await.unwrap();
    session.tray.show(asked()).await;
    let (member, signal) = heard(&mut signals).await;
    assert_eq!(member, "NewStatus");
    let status: String = signal.body().deserialize().unwrap();
    assert_eq!(status, "NeedsAttention");
    assert_eq!(heard(&mut signals).await.0, "NewToolTip");
    let (member, signal) = heard(&mut signals).await;
    assert_eq!(member, "XAyatanaNewLabel");
    let label: (String, String) = signal.body().deserialize().unwrap();
    assert_eq!(label, ("3".to_owned(), "3".to_owned()));
    let (member, signal) = heard(&mut signals).await;
    assert_eq!(member, "LayoutUpdated");
    let updated: (u32, i32) = signal.body().deserialize().unwrap();
    assert_eq!(updated, (2, 0));
}

#[tokio::test]
async fn an_item_that_only_changed_its_words_is_announced_by_its_id() {
    type Updated = Vec<(i32, HashMap<String, OwnedVariant>)>;
    type Removed = Vec<(i32, Vec<String>)>;
    let unread = |unread: usize| Menu {
        unread,
        ..signed_in()
    };
    let session = showing(unread(1)).await;
    let (_, menu) = session.host.menu().await.unwrap();
    let mut signals = session.host.signals().await.unwrap();
    // The same menu again changes nothing, so the host hears nothing of
    // it, and the next signal is of the menu after.
    session.tray.show(unread(1)).await;
    session.tray.show(unread(2)).await;
    let (member, signal) = heard(&mut signals).await;
    assert_eq!(member, "ItemsPropertiesUpdated");
    let (updated, removed): (Updated, Removed) = signal.body().deserialize().unwrap();
    assert_eq!(removed, []);
    let [(changed, properties)] = updated.as_slice() else {
        panic!("expected one item, got {updated:?}");
    };
    assert_eq!(*changed, id(&menu, "Open transom · 1 unread"));
    let label: &str = properties["label"].downcast_ref().unwrap();
    assert_eq!(label, "Open transom · 2 unread");
    let (member, signal) = heard(&mut signals).await;
    assert_eq!(member, "LayoutUpdated");
    let updated: (u32, i32) = signal.body().deserialize().unwrap();
    assert_eq!(updated, (3, 0));
}

#[tokio::test]
async fn activating_the_icon_either_way_opens_the_inbox_with_the_token() {
    let mut session = session().await;
    session.host.provide_activation_token("one").await.unwrap();
    session.host.activate().await.unwrap();
    session.host.provide_activation_token("two").await.unwrap();
    session.host.secondary_activate().await.unwrap();
    let token = |token: &str| Some(token.to_owned());
    assert_eq!(
        session.picked.try_recv(),
        Ok(Picked::Inbox {
            token: token("one")
        })
    );
    assert_eq!(
        session.picked.try_recv(),
        Ok(Picked::Inbox {
            token: token("two")
        })
    );
}

#[tokio::test]
async fn a_context_menu_and_a_scroll_do_nothing() {
    let mut session = session().await;
    let (item, _) = session.host.proxies();
    item.call::<_, _, ()>("ContextMenu", &(0, 0)).await.unwrap();
    let scroll = item.call::<_, _, ()>("Scroll", &(1, "vertical"));
    scroll.await.unwrap();
    assert_eq!(session.picked.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn an_installed_app_names_no_icon_directory() {
    let bus = TestBus::start();
    let desktop = desktop(&bus).await;
    let (_tray, _picked) = tray(&bus, "").await;
    let host = desktop.tray_item().await.unwrap();
    let icons: Vec<String> = host.menu_property("IconThemePath").await.unwrap();
    assert_eq!(icons, [""; 0]);
    assert_eq!(text(&host, "IconThemePath").await, "");
}
