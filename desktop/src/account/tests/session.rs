//! The account's session: starting from a kept one, signing in through
//! the browser, renaming, and the ways a session ends.

use serde_json::{Value, json};

use super::*;
use crate::settings::Preferences;
use crate::testserver::NOWHERE;

/// No one signed in, with nothing wrong and no sign-in under way.
const SIGNED_OUT: Phase = Phase::SignedOut {
    problem: None,
    waiting: false,
};

/// No one signed in because of `problem`.
fn because(problem: &str) -> Phase {
    Phase::SignedOut {
        problem: Some(problem.to_owned()),
        waiting: false,
    }
}

/// The account's phase once it tells a problem.
async fn troubled(app: &mut App) -> Phase {
    let troubled = |snapshot: &Arc<Snapshot>| matches!(&snapshot.phase, Phase::SignedOut { problem, .. } if problem.is_some());
    app.snapshot(troubled).await.phase.clone()
}

/// Each session the desktop's keyring holds, as JSON.
fn kept(app: &App) -> Vec<Value> {
    let kept = app.desktop.secrets.values();
    let parsed = kept
        .iter()
        .map(|kept| serde_json::from_slice(kept).unwrap());
    parsed.collect()
}

#[tokio::test]
async fn with_no_session_kept_no_one_is_signed_in() {
    let mut app = App::signed_out().await;
    let snapshot = app.snapshot(|_| true).await;
    assert_eq!(snapshot.phase, SIGNED_OUT);
    assert_eq!(snapshot.me(), None);
    assert_eq!(app.menu().await.lines(), ["Sign in…", "---", "Quit"]);
}

#[tokio::test]
async fn a_kept_session_is_signed_in_with_the_servers_list() {
    let before = async |pair: &Pair, _: &TestDesktop| {
        pair.ask("Restart?", &["Yes", "No"]).await;
    };
    let mut app = App::signed_in_after(before).await;
    app.summarized("Restart?").await;
    let snapshot = app.snapshot(|_| true).await;
    let signed_in = Phase::SignedIn {
        server: app.pair.server.url.clone(),
        me: "@bob".to_owned(),
    };
    assert_eq!(snapshot.phase, signed_in);
    assert_eq!(snapshot.me(), Some("@bob"));
    assert!(snapshot.connected && snapshot.heard.is_some());
    assert!(snapshot.client.is_some() && snapshot.pictures.is_some());
}

#[tokio::test]
async fn a_sign_in_waits_on_the_browser() {
    let mut app = App::signed_out().await;
    app.through_the_browser().await;
    let waiting = Phase::SignedOut {
        problem: None,
        waiting: true,
    };
    let snapshot = app.snapshot(|snapshot| snapshot.phase != SIGNED_OUT);
    assert_eq!(snapshot.await.phase, waiting);
}

#[tokio::test]
async fn the_browsers_callback_signs_in_and_shows_the_inbox() {
    let mut app = App::signed_out().await;
    let callback = app.through_the_browser().await;
    app.send(Command::Callback(callback));
    assert_eq!(app.show().await, Show::Inbox { token: None });
    let snapshot = app.snapshot(|snapshot| snapshot.loaded).await;
    assert_eq!(snapshot.me(), Some("@bob"));
}

#[tokio::test]
async fn a_sign_in_is_kept_in_the_keyring_and_the_preferences() {
    let mut app = App::signed_out().await;
    let callback = app.through_the_browser().await;
    app.send(Command::Callback(callback));
    app.snapshot(|snapshot| snapshot.loaded).await;
    let server = app.pair.server.url.to_string();
    assert_eq!(kept(&app)[0]["me"], "@bob");
    assert_eq!(kept(&app)[0]["server"], server);
    let preferences = Preferences::load(&app.places);
    assert_eq!(preferences.server, Some(server));
    assert!(preferences.since.is_some());
}

#[tokio::test]
async fn what_is_typed_that_is_no_address_is_told() {
    let mut app = App::signed_out().await;
    app.send(Command::SignIn("   ".to_owned()));
    let told = because("That is not a server's address.");
    assert_eq!(troubled(&mut app).await, told);
}

#[tokio::test]
async fn a_server_that_cannot_be_reached_is_told() {
    let mut app = App::signed_out().await;
    app.send(Command::SignIn(NOWHERE.to_owned()));
    let told = troubled(&mut app).await;
    assert!(matches!(
        told,
        Phase::SignedOut { problem: Some(problem), waiting: false }
            if problem.starts_with("The server could not be reached: ")
    ));
}

#[tokio::test]
async fn a_callback_the_server_refuses_is_told() {
    let mut app = App::signed_out().await;
    app.through_the_browser().await;
    let forged = "sh.transom:/callback?code=forged".to_owned();
    app.send(Command::Callback(forged));
    let told = because("The server refused the sign-in. Try again.");
    assert_eq!(troubled(&mut app).await, told);
}

#[tokio::test]
async fn a_callback_no_sign_in_waits_for_is_told() {
    let mut app = App::signed_out().await;
    let callback = "sh.transom:/callback?code=abc".to_owned();
    app.send(Command::Callback(callback));
    let told = because("That sign-in was not started here. Try again.");
    assert_eq!(troubled(&mut app).await, told);
}

#[tokio::test]
async fn a_callback_without_a_code_is_told_and_the_sign_in_still_waits() {
    let mut app = App::signed_out().await;
    app.through_the_browser().await;
    app.send(Command::Callback("sh.transom:/callback".to_owned()));
    let told = Phase::SignedOut {
        problem: Some("That sign-in was not started here. Try again.".to_owned()),
        waiting: true,
    };
    assert_eq!(troubled(&mut app).await, told);
}

#[tokio::test]
async fn a_kept_session_the_server_no_longer_takes_is_ended() {
    let before = async |pair: &Pair, desktop: &TestDesktop| {
        keep(desktop, &pair.server.url, "not-a-token").await;
    };
    let mut app = App::start_after(before).await;
    let told = because("Your session ended. Sign in again.");
    assert_eq!(troubled(&mut app).await, told);
    assert_eq!(kept(&app), Vec::<Value>::new());
}

#[tokio::test]
async fn signing_out_forgets_the_session_and_this_device() {
    let mut app = App::signed_in().await;
    // The feed sends what changes only after this device is registered.
    let id = app.pair.tell("Backup finished").await;
    app.entry(&id, |_| true).await;
    app.send(Command::SignOut);
    let snapshot = app.snapshot(|snapshot| snapshot.me().is_none()).await;
    assert_eq!(snapshot.phase, SIGNED_OUT);
    assert_eq!(kept(&app), Vec::<Value>::new());
    let devices = app.pair.client().devices("@bob").await.unwrap();
    assert_eq!(devices, []);
    assert_eq!(Preferences::load(&app.places).since, None);
}

#[tokio::test]
async fn signing_out_empties_the_inbox_the_notifications_and_the_tray() {
    let mut app = App::signed_in().await;
    let id = app.pair.ask("Restart?", &["Yes", "No"]).await;
    app.notification(&id, "Restart?").await;
    app.send(Command::SignOut);
    let snapshot = app.snapshot(|snapshot| snapshot.me().is_none()).await;
    assert!(snapshot.entries.is_empty() && !snapshot.loaded);
    assert_eq!(app.desktop.notifications.showing(), []);
    assert_eq!(app.menu().await.lines(), ["Sign in…", "---", "Quit"]);
}

#[tokio::test]
async fn a_new_handle_is_followed_and_kept() {
    let mut app = App::signed_in().await;
    let renamed = json!({ "name": "robert" });
    app.pair.client().change("@bob", renamed).await.unwrap();
    app.send(Command::Renamed("@robert".to_owned()));
    app.snapshot(|snapshot| snapshot.me() == Some("@robert"))
        .await;
    assert_eq!(kept(&app)[0]["me"], "@robert");
    let (server, alice) = (&app.pair.server, &app.pair.alice);
    let message = json!({ "summary": "Hello, Robert" });
    let id = server.send(alice, "@robert", message).await;
    app.entry(&id, |_| true).await;
}

#[tokio::test]
async fn a_new_handle_means_nothing_while_no_one_is_signed_in() {
    let mut app = App::signed_out().await;
    app.send(Command::Renamed("@robert".to_owned()));
    app.send(Command::SignIn(String::new()));
    troubled(&mut app).await;
    assert_eq!(kept(&app), Vec::<Value>::new());
}

#[tokio::test]
async fn signing_out_while_no_one_is_signed_in_tells_no_problem() {
    let mut app = App::signed_out().await;
    app.send(Command::SignIn(String::new()));
    troubled(&mut app).await;
    app.send(Command::SignOut);
    let snapshot = app.snapshot(|snapshot| snapshot.phase == SIGNED_OUT);
    snapshot.await;
}
