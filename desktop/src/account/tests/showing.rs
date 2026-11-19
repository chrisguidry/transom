//! What the account shows of the inbox outside the window: the desktop's
//! notifications and the tray's icon and menu, as the server's list
//! changes, as the connection comes and goes, and on a desktop that
//! lacks a service.

use serde_json::json;

use super::*;
use crate::exchange::State;
use crate::settings::Preferences;

/// The tray's menu while `@alice`'s question "Restart?" waits.
const WAITING: [&str; 11] = [
    "(Waiting on you)",
    "@alice: Restart? >",
    "  Yes",
    "  No",
    "  ---",
    "  Open",
    "---",
    "Open transom",
    "[x] Notifications",
    "---",
    "Quit",
];

#[tokio::test]
async fn a_question_that_arrives_is_notified_with_a_sound_and_its_choices() {
    let mut app = App::signed_in().await;
    let id = app.pair.ask("Restart?", &["Yes", "No"]).await;
    let posted = app.notification(&id, "Restart?").await;
    assert_eq!(posted.summary, "@alice");
    assert_eq!(posted.sound.as_deref(), Some("message-new-instant"));
    let labels: Vec<&str> = posted.actions.iter().map(|(_, label)| &**label).collect();
    assert_eq!(labels, ["Open", "Yes", "No"]);
}

#[tokio::test]
async fn a_question_that_waits_is_in_the_trays_menu_and_beside_its_icon() {
    let mut app = App::signed_in().await;
    let id = app.pair.ask("Restart?", &["Yes", "No"]).await;
    app.entry(&id, |_| true).await;
    assert_eq!(app.menu().await.lines(), WAITING);
    let tray = app.tray().await;
    let status: String = tray.property("Status").await.unwrap();
    let count: String = tray.property("XAyatanaLabel").await.unwrap();
    assert_eq!((status.as_str(), count.as_str()), ("NeedsAttention", "1"));
}

#[tokio::test]
async fn a_question_that_waited_before_the_app_started_is_notified_without_a_sound() {
    let before = async |pair: &Pair, _: &TestDesktop| {
        pair.ask("Restart?", &["Yes", "No"]).await;
    };
    let app = App::signed_in_after(before).await;
    assert_eq!(app.posted("Restart?").unwrap().sound, None);
}

#[tokio::test]
async fn a_notification_from_before_the_session_is_unread_and_not_notified() {
    let before = async |pair: &Pair, _: &TestDesktop| {
        pair.tell("Backup finished").await;
    };
    let app = App::signed_in_after(before).await;
    assert_eq!(app.desktop.notifications.showing(), []);
    let menu = app.menu().await;
    assert!(menu.find("Open transom · 1 unread").is_some());
}

#[tokio::test]
async fn the_senders_picture_is_the_notifications_image() {
    let mut app = App::signed_in().await;
    app.pair.pictured("Backup finished").await;
    app.summarized("Backup finished").await;
    let image = app.posted("Backup finished").unwrap().image_path.unwrap();
    assert!(image.starts_with(app.places.cache.join("pictures").to_str().unwrap()));
}

#[tokio::test]
async fn an_exchange_read_on_another_device_has_its_notification_taken_down() {
    let mut app = App::signed_in().await;
    let id = app.pair.ask("Restart?", &["Yes", "No"]).await;
    app.notification(&id, "Restart?").await;
    let client = app.pair.client();
    client.mark(&id, "@bob", true).await.unwrap();
    app.entry(&id, |entry| entry.read == Some(true)).await;
    assert_eq!(app.desktop.notifications.showing(), []);
}

#[tokio::test]
async fn a_message_that_replaces_one_takes_over_its_notification() {
    let mut app = App::signed_in().await;
    let (server, alice) = (&app.pair.server, &app.pair.alice);
    let firing = json!({ "summary": "Disk at 91%", "key": "disk" });
    let first = server.send(alice, "@bob", firing).await;
    let earlier = app.notification(&first, "Disk at 91%").await;
    let (server, alice) = (&app.pair.server, &app.pair.alice);
    let worse = json!({ "summary": "Disk at 97%", "key": "disk" });
    let second = server.send(alice, "@bob", worse).await;
    let later = app.notification(&second, "Disk at 97%").await;
    assert_eq!((later.id, later.posts), (earlier.id, 2));
    assert_eq!(app.desktop.notifications.showing().len(), 1);
}

#[tokio::test]
async fn turning_notifications_off_takes_every_one_down_and_is_kept() {
    let mut app = App::signed_in().await;
    let id = app.pair.ask("Restart?", &["Yes", "No"]).await;
    app.notification(&id, "Restart?").await;
    app.send(Command::Notifications(false));
    app.snapshot(|snapshot| !snapshot.preferences.notifications)
        .await;
    assert_eq!(app.desktop.notifications.showing(), []);
    assert!(!Preferences::load(&app.places).notifications);
    assert!(
        app.menu()
            .await
            .lines()
            .contains(&"[ ] Notifications".to_owned())
    );
}

#[tokio::test]
async fn turning_notifications_on_again_notifies_what_is_unread() {
    let mut app = App::signed_in().await;
    let id = app.pair.ask("Restart?", &["Yes", "No"]).await;
    app.notification(&id, "Restart?").await;
    app.send(Command::Notifications(false));
    app.send(Command::Notifications(true));
    app.snapshot(|snapshot| snapshot.preferences.notifications)
        .await;
    assert!(app.posted("Restart?").is_some());
}

#[tokio::test]
async fn the_trays_toggle_turns_notifications_off() {
    let mut app = App::signed_in().await;
    app.click(&["Notifications"]).await;
    app.snapshot(|snapshot| !snapshot.preferences.notifications)
        .await;
    assert!(!Preferences::load(&app.places).notifications);
}

#[tokio::test]
async fn a_server_that_went_away_is_said_in_the_window_and_the_tray() {
    let mut app = App::signed_in().await;
    app.pair.server.stop();
    let snapshot = app.snapshot(|snapshot| !snapshot.connected).await;
    assert!(snapshot.loaded && snapshot.heard.is_some());
    let menu = app.menu().await;
    assert!(menu.find("Not connected to the server").is_some());
}

#[tokio::test]
async fn an_answer_made_while_the_server_is_away_is_unsent_until_it_is_back() {
    let mut app = App::signed_in().await;
    let id = app.pair.ask("Restart?", &["Yes", "No"]).await;
    app.entry(&id, |_| true).await;
    app.pair.server.stop();
    app.snapshot(|snapshot| !snapshot.connected).await;
    let yes = Command::Reply {
        exchange: id.clone(),
        choice: Some("Yes".to_owned()),
        text: None,
    };
    app.send(yes);
    let unsent = app.entry(&id, |entry| entry.sending.is_some()).await;
    assert_eq!(unsent.read, Some(true));
    let menu = app.menu().await;
    assert!(
        menu.find("Restart?: Your answer, Yes, is not sent yet")
            .is_some()
    );
    app.pair.server.restart().await;
    app.send(Command::Woke);
    let replied = |entry: &Entry| entry.exchange.state == State::Replied;
    assert_eq!(app.entry(&id, replied).await.sending, None);
    assert_eq!(app.on_server(&id).await["reply"]["choice"], "Yes");
}

#[tokio::test]
async fn a_wake_connects_to_the_server_again() {
    let mut app = App::signed_in().await;
    let before = app.snapshot(|_| true).await.heard;
    app.send(Command::Woke);
    let again = |snapshot: &Arc<Snapshot>| snapshot.connected && snapshot.heard > before;
    app.snapshot(again).await;
}

#[tokio::test]
async fn an_exchange_the_server_deleted_leaves_the_inbox() {
    let before = async |pair: &Pair, _: &TestDesktop| {
        let (server, bob) = (&pair.server, &pair.bob);
        let made = server.call(reqwest::Method::PUT, "/@team", bob, json!(null));
        made.await;
        server
            .send(bob, "@team", json!({ "summary": "Lunch" }))
            .await;
    };
    let mut app = App::signed_in_after(before).await;
    app.summarized("Lunch").await;
    let client = app.pair.client();
    client.remove("@team", "@bob").await.unwrap();
    app.snapshot(|snapshot| has(snapshot, "Lunch").is_none())
        .await;
}

#[tokio::test]
async fn a_build_that_is_not_installed_names_its_icons_to_the_trays_host() {
    let bus = TestBus::start();
    let desktop = TestDesktop::serve(bus.connect().await.unwrap());
    let desktop = desktop.await.unwrap();
    let (_commands, heard_commands) = unbounded_channel();
    let (snapshot_sender, _snapshots) = watch::channel(Arc::default());
    let (show_sender, _shows) = unbounded_channel();
    let window = (heard_commands, snapshot_sender, show_sender);
    let session = Some(bus.connect().await.unwrap());
    let icons = Some("/source/data/icons");
    tokio::spawn(desktop::run(Places::fresh(), session, icons, window));
    let tray = tokio::time::timeout(SOON, desktop.tray_item());
    let tray = tray.await.expect("the tray registers").unwrap();
    let named: String = tray.property("IconThemePath").await.unwrap();
    assert_eq!(named, "/source/data/icons/hicolor/symbolic/apps");
}

#[tokio::test]
async fn without_a_session_bus_the_account_still_signs_in_and_lists() {
    let before = async |pair: &Pair, _: &TestDesktop| {
        pair.ask("Restart?", &["Yes", "No"]).await;
    };
    let mut app = App::start_on(async |_| None, before).await;
    let callback = app.through_the_browser().await;
    app.send(Command::Callback(callback));
    app.summarized("Restart?").await;
    assert_eq!(app.desktop.notifications.showing(), []);
    app.send(Command::Notifications(false));
    app.snapshot(|snapshot| !snapshot.preferences.notifications)
        .await;
    assert!(!Preferences::load(&app.places).notifications);
}

#[tokio::test]
async fn without_a_session_bus_signing_out_still_signs_out() {
    let mut app = App::start_on(async |_| None, async |_, _| {}).await;
    let callback = app.through_the_browser().await;
    app.send(Command::Callback(callback));
    app.snapshot(|snapshot| snapshot.loaded).await;
    app.send(Command::SignOut);
    let snapshot = app.snapshot(|snapshot| snapshot.me().is_none()).await;
    assert_eq!(snapshot.entries, Vec::<Entry>::new());
}

#[tokio::test]
async fn on_a_desktop_without_a_keyring_a_sign_in_still_signs_in() {
    let bare = TestBus::start();
    let on_bare = async |_: &TestBus| Some(bare.connect().await.unwrap());
    let mut app = App::start_on(on_bare, async |_, _| {}).await;
    let callback = app.through_the_browser().await;
    app.send(Command::Callback(callback));
    let snapshot = app.snapshot(|snapshot| snapshot.loaded).await;
    assert_eq!(snapshot.me(), Some("@bob"));
}
