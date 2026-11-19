//! An answer made here after someone else's: the first reply wins, and
//! the account says so where the person will see it.

use super::*;
use crate::exchange::State;
use crate::inbox::Sending;

/// What the lost reply's notification says.
const NOT_USED: &str = "Your answer, No, was not used: @bob answered first";

fn no(id: &str) -> Command {
    Command::Reply {
        exchange: id.to_owned(),
        choice: Some("No".to_owned()),
        text: None,
    }
}

fn lost_no() -> Sending {
    Sending {
        choice: Some("No".to_owned()),
        text: None,
    }
}

impl App {
    /// Has `@alice` ask `@bob` "Restart?", and `@bob` answer Yes on
    /// another device. Returns the exchange's id and the notification the
    /// desktop showed for it, once the account has the exchange as
    /// replied and has taken the notification down.
    async fn answered_elsewhere(&mut self) -> (String, Posted) {
        let id = self.pair.ask("Restart?", &["Yes", "No"]).await;
        let posted = self.notification(&id, "Restart?").await;
        self.pair.server.reply(&self.pair.bob, &id, "Yes").await;
        let replied = |entry: &Entry| entry.exchange.state == State::Replied;
        self.entry(&id, replied).await;
        (id, posted)
    }

    /// As `answered_elsewhere`, and then `@bob` presses No on the
    /// notification in the moment after the account took it down.
    /// Returns the exchange's id and the notification that says the
    /// answer was not used.
    async fn lost_from_a_notification(&mut self) -> (String, Posted) {
        let (id, posted) = self.answered_elsewhere().await;
        self.desktop.invoke(posted.id, "choice:No").await.unwrap();
        self.entry(&id, |entry| entry.lost.is_some()).await;
        (id, self.posted(NOT_USED).unwrap())
    }
}

#[tokio::test]
async fn an_answer_from_the_window_that_came_second_is_shown_as_lost() {
    let mut app = App::signed_in().await;
    let (id, _) = app.answered_elsewhere().await;
    app.send(no(&id));
    let entry = app.entry(&id, |entry| entry.lost.is_some()).await;
    assert_eq!(entry.lost, Some(lost_no()));
    assert_eq!(entry.exchange.reply.unwrap().choice.unwrap(), "Yes");
    assert_eq!(app.desktop.notifications.showing(), []);
}

#[tokio::test]
async fn the_person_having_seen_a_lost_answer_ends_it() {
    let mut app = App::signed_in().await;
    let (id, _) = app.answered_elsewhere().await;
    app.send(no(&id));
    app.entry(&id, |entry| entry.lost.is_some()).await;
    app.send(Command::Seen(id.clone()));
    app.entry(&id, |entry| entry.lost.is_none()).await;
}

#[tokio::test]
async fn an_answer_from_a_notification_that_came_second_is_said_in_a_notification() {
    let mut app = App::signed_in().await;
    let (_, said) = app.lost_from_a_notification().await;
    assert_eq!(said.summary, "@alice");
    assert_eq!(said.body, format!("<b>Restart?</b> {NOT_USED}"));
    assert_eq!(said.actions, [("default".to_owned(), "Open".to_owned())]);
}

#[tokio::test]
async fn dismissing_the_notification_of_a_lost_answer_ends_it() {
    let mut app = App::signed_in().await;
    let (id, said) = app.lost_from_a_notification().await;
    app.desktop.dismiss(said.id).await.unwrap();
    app.entry(&id, |entry| entry.lost.is_none()).await;
}

#[tokio::test]
async fn pressing_the_notification_of_a_lost_answer_opens_its_exchange() {
    let mut app = App::signed_in().await;
    let (id, said) = app.lost_from_a_notification().await;
    app.desktop.invoke(said.id, "default").await.unwrap();
    let shown = Show::Exchange {
        id,
        token: Some("test-activation-token".to_owned()),
        reply: false,
    };
    assert_eq!(app.show().await, shown);
}

#[tokio::test]
async fn the_person_having_seen_a_lost_answer_takes_its_notification_down() {
    let mut app = App::signed_in().await;
    let (id, _) = app.lost_from_a_notification().await;
    app.send(Command::Seen(id.clone()));
    app.entry(&id, |entry| entry.lost.is_none()).await;
    assert_eq!(app.desktop.notifications.showing(), []);
}

#[tokio::test]
async fn signing_out_takes_the_notification_of_a_lost_answer_down() {
    let mut app = App::signed_in().await;
    app.lost_from_a_notification().await;
    app.send(Command::SignOut);
    app.snapshot(|snapshot| snapshot.me().is_none()).await;
    assert_eq!(app.desktop.notifications.showing(), []);
}

#[tokio::test]
async fn an_answer_picked_in_the_tray_while_away_that_came_second_is_said_in_a_notification() {
    let mut app = App::signed_in().await;
    let id = app.pair.ask("Restart?", &["Yes", "No"]).await;
    app.entry(&id, |_| true).await;
    app.pair.server.stop();
    app.snapshot(|snapshot| !snapshot.connected).await;
    app.click(&["@alice: Restart?", "No"]).await;
    app.entry(&id, |entry| entry.sending.is_some()).await;
    app.pair.server.restart().await;
    app.pair.server.reply(&app.pair.bob, &id, "Yes").await;
    app.send(Command::Woke);
    let entry = app.entry(&id, |entry| entry.lost.is_some()).await;
    assert_eq!((entry.lost, entry.sending), (Some(lost_no()), None));
    assert!(app.posted(NOT_USED).is_some());
}

#[tokio::test]
async fn having_seen_an_exchange_whose_answer_was_used_changes_nothing() {
    let mut app = App::signed_in().await;
    let id = app.pair.ask("Restart?", &["Yes", "No"]).await;
    app.notification(&id, "Restart?").await;
    app.send(Command::Seen(id.clone()));
    app.send(Command::Mark {
        exchanges: vec![id.clone()],
        read: true,
    });
    let entry = app.entry(&id, |entry| entry.read == Some(true)).await;
    assert_eq!(entry.lost, None);
}
