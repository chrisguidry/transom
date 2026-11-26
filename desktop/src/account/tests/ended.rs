//! A session the server ended: any call it refuses signs the account out
//! once, drops what was queued, and says so in a notification, and a
//! refusal the account expects, or one from a session already over,
//! changes nothing.

use super::*;
use crate::settings::Preferences;
use crate::sync::outbox::Outbox;

/// What the signed-out notification's body starts with.
const SIGN_IN_AGAIN: &str = "Sign in again to get your notifications.";

impl App {
    /// The account signed in through the browser, so it holds a session
    /// of transom's own, as a person's app does, once it has the list.
    async fn with_a_session() -> Self {
        let mut app = Self::signed_out().await;
        app.sign_in_again().await;
        app
    }

    async fn sign_in_again(&mut self) {
        let callback = self.through_the_browser().await;
        self.send(Command::Callback(callback));
        assert_eq!(self.show().await, Show::Inbox { token: None });
        self.snapshot(|snapshot| snapshot.loaded).await;
    }

    /// Ends the account's session on the server, and has a screen ask
    /// the server something with it.
    async fn ended_on_the_server(&mut self) {
        let token = kept(self)[0]["token"].as_str().unwrap().to_owned();
        self.pair.server.end_session(&token).await;
        let client = self.snapshot(|_| true).await.client.clone().unwrap();
        client.devices("@bob").await.unwrap_err();
    }

    fn outbox(&self) -> Outbox {
        Outbox::open(&self.places.state.join("outbox.json"))
    }
}

#[tokio::test]
async fn a_session_the_server_ended_signs_out_and_says_so() {
    let mut app = App::with_a_session().await;
    app.ended_on_the_server().await;
    let told = because("Your session ended. Sign in again.");
    assert_eq!(troubled(&mut app).await, told);
    let posted = app.posted(SIGN_IN_AGAIN).unwrap();
    assert_eq!(posted.summary, "transom signed you out");
    assert_eq!(app.desktop.notifications.showing(), [posted]);
    assert_eq!(kept(&app), Vec::<serde_json::Value>::new());
    let server = Some(app.pair.server.url.to_string());
    assert_eq!(Preferences::load(&app.places).server, server);
}

#[tokio::test]
async fn replies_queued_when_the_session_ended_are_dropped_and_counted() {
    let mut app = App::with_a_session().await;
    let id = app.pair.ask("Restart?", &["Yes", "No"]).await;
    app.notification(&id, "Restart?").await;
    let token = kept(&app)[0]["token"].as_str().unwrap().to_owned();
    app.pair.server.end_session(&token).await;
    app.send(Command::Reply {
        exchange: id.clone(),
        choice: Some("Yes".to_owned()),
        text: None,
    });
    troubled(&mut app).await;
    let posted = app.posted("1 reply wasn't sent.").unwrap();
    assert_eq!(app.desktop.notifications.showing(), [posted]);
    assert_eq!(app.outbox().queued(), []);
    assert_eq!(app.on_server(&id).await["state"], "open");
}

#[tokio::test]
async fn with_notifications_off_a_session_the_server_ended_posts_nothing() {
    let mut app = App::with_a_session().await;
    app.send(Command::Notifications(false));
    app.snapshot(|snapshot| !snapshot.preferences.notifications)
        .await;
    app.ended_on_the_server().await;
    troubled(&mut app).await;
    assert_eq!(app.desktop.notifications.showing(), []);
}

#[tokio::test]
async fn pressing_the_signed_out_notification_shows_the_sign_in_window() {
    let mut app = App::with_a_session().await;
    app.ended_on_the_server().await;
    troubled(&mut app).await;
    let posted = app.posted(SIGN_IN_AGAIN).unwrap();
    app.desktop.invoke(posted.id, "default").await.unwrap();
    let token = Some("test-activation-token".to_owned());
    assert_eq!(app.show().await, Show::Inbox { token });
}

#[tokio::test]
async fn dismissing_the_signed_out_notification_queues_nothing() {
    let mut app = App::with_a_session().await;
    app.ended_on_the_server().await;
    troubled(&mut app).await;
    let posted = app.posted(SIGN_IN_AGAIN).unwrap();
    app.snapshots.mark_unchanged();
    app.desktop.dismiss(posted.id).await.unwrap();
    let published = tokio::time::timeout(SOON, app.snapshots.changed());
    published.await.unwrap().unwrap();
    assert_eq!(app.outbox().queued(), []);
}

#[tokio::test]
async fn signing_in_again_takes_the_signed_out_notification_down() {
    let mut app = App::with_a_session().await;
    app.ended_on_the_server().await;
    troubled(&mut app).await;
    app.sign_in_again().await;
    assert_eq!(app.posted(SIGN_IN_AGAIN), None);
}

#[tokio::test]
async fn signing_out_here_tells_no_problem_and_posts_nothing() {
    let mut app = App::with_a_session().await;
    app.send(Command::SignOut);
    let snapshot = app.snapshot(|snapshot| snapshot.me().is_none()).await;
    assert_eq!(snapshot.phase, SIGNED_OUT);
    // Logging out after this device was forgotten draws a 401, which the
    // account hears after it signed out.
    app.send(Command::SignIn(String::new()));
    let told = because("That is not a server's address.");
    assert_eq!(troubled(&mut app).await, told);
    assert_eq!(app.desktop.notifications.showing(), []);
}

#[tokio::test]
async fn a_session_already_over_does_not_end_the_next_one() {
    let mut app = App::with_a_session().await;
    let earlier = app.snapshot(|_| true).await.client.clone().unwrap();
    app.send(Command::SignOut);
    app.snapshot(|snapshot| snapshot.me().is_none()).await;
    app.sign_in_again().await;
    earlier.devices("@bob").await.unwrap_err();
    // The feed's news comes after the earlier session's report.
    let id = app.pair.tell("Backup finished").await;
    app.entry(&id, |_| true).await;
    assert_eq!(app.snapshot(|_| true).await.me(), Some("@bob"));
    assert_eq!(app.posted(SIGN_IN_AGAIN), None);
}
