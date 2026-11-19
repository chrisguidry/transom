//! What a person does to an exchange from the window, a notification,
//! and the tray: answering, reading, opening, and withdrawing.

use serde_json::json;

use super::*;
use crate::exchange::{Exchange, State};

/// The token `test-desktop` gives with a press on a notification.
const PRESS_TOKEN: &str = "test-activation-token";

/// The token the harness gives with a click on the tray.
const CLICK_TOKEN: &str = "tray-token";

impl App {
    /// Has `@alice` ask `@bob` "Restart?", to answer Yes or No, and
    /// returns the exchange's id once the account has it and its
    /// notification is posted.
    async fn asked(&mut self) -> String {
        let id = self.pair.ask("Restart?", &["Yes", "No"]).await;
        self.entry(&id, |_| true).await;
        id
    }

    /// Has `@alice` send `@bob` `message`, and returns the exchange's id
    /// and the notification the desktop shows for it.
    async fn sent(&mut self, message: serde_json::Value) -> (String, Posted) {
        let text = message["summary"].as_str().unwrap().to_owned();
        let (server, alice) = (&self.pair.server, &self.pair.alice);
        let id = server.send(alice, "@bob", message).await;
        let posted = self.notification(&id, &text).await;
        (id, posted)
    }

    /// The server's reply to exchange `id`, once the account has it as
    /// replied.
    async fn replied(&mut self, id: &str) -> serde_json::Value {
        let replied = |entry: &Entry| entry.exchange.state == State::Replied;
        self.entry(id, replied).await;
        self.on_server(id).await["reply"].clone()
    }

    /// An exchange the account is asked to show, with `token`, and
    /// whether with its text box ready.
    fn exchange_shown(id: &str, token: &str, reply: bool) -> Show {
        Show::Exchange {
            id: id.to_owned(),
            token: Some(token.to_owned()),
            reply,
        }
    }
}

fn answer(id: &str, choice: Option<&str>, text: Option<&str>) -> Command {
    Command::Reply {
        exchange: id.to_owned(),
        choice: choice.map(str::to_owned),
        text: text.map(str::to_owned),
    }
}

#[tokio::test]
async fn a_choice_made_in_the_window_is_the_exchanges_reply() {
    let mut app = App::signed_in().await;
    let id = app.asked().await;
    app.send(answer(&id, Some("Yes"), None));
    assert_eq!(app.replied(&id).await["choice"], "Yes");
    assert_eq!(app.entry(&id, |_| true).await.read, Some(true));
}

#[tokio::test]
async fn text_typed_in_the_window_is_the_exchanges_reply() {
    let mut app = App::signed_in().await;
    let question = json!({ "summary": "Why?", "prompt": "Say why" });
    let (id, _) = app.sent(question).await;
    app.send(answer(&id, None, Some("Because")));
    assert_eq!(app.replied(&id).await["text"], "Because");
}

#[tokio::test]
async fn a_choice_pressed_on_a_notification_is_the_exchanges_reply() {
    let mut app = App::signed_in().await;
    let id = app.asked().await;
    let posted = app.posted("Restart?").unwrap();
    app.desktop.invoke(posted.id, "choice:Yes").await.unwrap();
    assert_eq!(app.replied(&id).await["choice"], "Yes");
}

#[tokio::test]
async fn a_choice_picked_in_the_tray_is_the_exchanges_reply() {
    let mut app = App::signed_in().await;
    let id = app.asked().await;
    app.click(&["@alice: Restart?", "No"]).await;
    assert_eq!(app.replied(&id).await["choice"], "No");
}

#[tokio::test]
async fn got_it_on_an_urgent_notification_reads_the_exchange() {
    let mut app = App::signed_in().await;
    let mut phone = app.pair.elsewhere().await;
    let urgent = json!({ "summary": "Disk full", "urgency": "urgent" });
    let (id, posted) = app.sent(urgent).await;
    app.desktop.invoke(posted.id, "got-it").await.unwrap();
    let read = |exchange: &Exchange| exchange.id == id && exchange.read == Some(true);
    phone.hears(read).await;
}

#[tokio::test]
async fn dismissing_a_notification_reads_the_exchange() {
    let mut app = App::signed_in().await;
    let mut phone = app.pair.elsewhere().await;
    let id = app.asked().await;
    let posted = app.posted("Restart?").unwrap();
    app.desktop.dismiss(posted.id).await.unwrap();
    let read = |exchange: &Exchange| exchange.id == id && exchange.read == Some(true);
    phone.hears(read).await;
}

#[tokio::test]
async fn pressing_a_notification_opens_its_exchange() {
    let mut app = App::signed_in().await;
    let id = app.asked().await;
    let posted = app.posted("Restart?").unwrap();
    app.desktop.invoke(posted.id, "default").await.unwrap();
    let shown = App::exchange_shown(&id, PRESS_TOKEN, false);
    assert_eq!(app.show().await, shown);
}

#[tokio::test]
async fn reply_on_a_notification_opens_its_exchange_at_the_text_box() {
    let mut app = App::signed_in().await;
    let question = json!({ "summary": "Why?", "prompt": "Say why" });
    let (id, posted) = app.sent(question).await;
    app.desktop.invoke(posted.id, "reply").await.unwrap();
    let shown = App::exchange_shown(&id, PRESS_TOKEN, true);
    assert_eq!(app.show().await, shown);
}

#[tokio::test]
async fn answer_on_a_notification_with_many_choices_opens_its_exchange() {
    let mut app = App::signed_in().await;
    let choices = ["North", "East", "South", "West"];
    let question = json!({ "summary": "Which way?", "choices": choices });
    let (id, posted) = app.sent(question).await;
    app.desktop.invoke(posted.id, "answer").await.unwrap();
    let shown = App::exchange_shown(&id, PRESS_TOKEN, false);
    assert_eq!(app.show().await, shown);
}

#[tokio::test]
async fn a_link_on_a_notification_opens_in_the_browser_and_reads_the_exchange() {
    let mut app = App::signed_in().await;
    let link = json!({ "href": "https://ci.example.com/1", "title": "Logs" });
    let failed = json!({ "summary": "Deploy failed", "link": [link] });
    let (id, posted) = app.sent(failed).await;
    let mut phone = app.pair.elsewhere().await;
    let pressed = "link:https://ci.example.com/1";
    app.desktop.invoke(posted.id, pressed).await.unwrap();
    let opened = Show::Browser("https://ci.example.com/1".to_owned());
    assert_eq!(app.show().await, opened);
    let read = |exchange: &Exchange| exchange.id == id && exchange.read == Some(true);
    phone.hears(read).await;
}

#[tokio::test]
async fn reply_in_the_tray_opens_the_exchange_at_the_text_box() {
    let mut app = App::signed_in().await;
    let question = json!({ "summary": "Why?", "prompt": "Say why" });
    let (id, _) = app.sent(question).await;
    app.click(&["@alice: Why?", "Reply…"]).await;
    let shown = App::exchange_shown(&id, CLICK_TOKEN, true);
    assert_eq!(app.show().await, shown);
}

#[tokio::test]
async fn open_in_the_tray_opens_the_exchange() {
    let mut app = App::signed_in().await;
    let id = app.asked().await;
    app.click(&["@alice: Restart?", "Open"]).await;
    let shown = App::exchange_shown(&id, CLICK_TOKEN, false);
    assert_eq!(app.show().await, shown);
}

#[tokio::test]
async fn open_transom_in_the_tray_opens_the_inbox() {
    let mut app = App::signed_in().await;
    app.click(&["Open transom"]).await;
    let token = Some(CLICK_TOKEN.to_owned());
    assert_eq!(app.show().await, Show::Inbox { token });
}

#[tokio::test]
async fn activating_the_trays_icon_opens_the_inbox() {
    let mut app = App::signed_in().await;
    app.tray().await.activate().await.unwrap();
    assert_eq!(app.show().await, Show::Inbox { token: None });
}

#[tokio::test]
async fn quit_in_the_tray_ends_the_account() {
    let mut app = App::signed_in().await;
    app.click(&["Quit"]).await;
    assert_eq!(app.show().await, Show::Quit);
    let ended = tokio::time::timeout(SOON, &mut app.task);
    ended.await.expect("the account ends").unwrap();
}

#[tokio::test]
async fn an_exchange_is_marked_read_and_unread_from_the_window() {
    let mut app = App::signed_in().await;
    let id = app.asked().await;
    let mut phone = app.pair.elsewhere().await;
    let mark = |read| Command::Mark {
        exchanges: vec![id.clone()],
        read,
    };
    app.send(mark(true));
    let read = app.entry(&id, |entry| entry.read == Some(true)).await;
    phone
        .hears(|exchange| exchange.id == id && exchange.read == Some(true))
        .await;
    app.send(mark(false));
    let unread = app.entry(&id, |entry| entry.read == Some(false)).await;
    phone
        .hears(|exchange| exchange.id == id && exchange.read == Some(false))
        .await;
    assert_eq!((read.read, unread.read), (Some(true), Some(false)));
}

#[tokio::test]
async fn a_message_this_person_sent_is_withdrawn() {
    let mut app = App::signed_in().await;
    let (server, bob) = (&app.pair.server, &app.pair.bob);
    let message = json!({ "summary": "Lunch?", "choices": ["Yes", "No"] });
    let id = server.send(bob, "@alice", message).await;
    app.entry(&id, |_| true).await;
    app.send(Command::Withdraw(id.clone()));
    let withdrawn = |entry: &Entry| entry.exchange.state == State::Withdrawn;
    app.entry(&id, withdrawn).await;
    assert_eq!(app.on_server(&id).await["state"], "withdrawn");
}

#[tokio::test]
async fn a_withdrawal_the_server_refuses_is_said() {
    let mut app = App::signed_in().await;
    let id = app.asked().await;
    app.send(Command::Withdraw(id));
    let refused = "transom answered 403: only the sender may withdraw a message";
    assert_eq!(app.show().await, Show::Notice(refused.to_owned()));
}

#[tokio::test]
async fn an_answer_the_server_refuses_for_good_is_said_and_not_sent_again() {
    let mut app = App::signed_in().await;
    let id = app.asked().await;
    let alice = app.pair.server.client(&app.pair.alice);
    alice.withdraw(&id).await.unwrap();
    app.send(answer(&id, Some("Yes"), None));
    let refused = "transom answered 409: the exchange is withdrawn";
    assert_eq!(app.show().await, Show::Notice(refused.to_owned()));
    app.entry(&id, |entry| entry.sending.is_none()).await;
    assert_eq!(app.menu().await.find("Not sent yet"), None);
}
