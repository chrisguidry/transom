//! The notifier on a private bus, with `test-desktop` as the desktop's
//! notification server: each test reads what the server was sent, and
//! presses and dismisses as a person would.

use std::time::Duration;

use test_desktop::notifications::{GNOME, Posted};
use test_desktop::{TestBus, TestDesktop};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::*;
use crate::settings::Places;

/// The notifier, the desktop it posts to, and what people did with the
/// notifications, on a bus of their own.
struct Session {
    notifier: Freedesktop,
    events: UnboundedReceiver<Event>,
    desktop: TestDesktop,
    places: Places,
    bus: TestBus,
}

impl Session {
    /// The one notification the desktop shows.
    fn posted(&self) -> Posted {
        let mut showing = self.desktop.notifications.showing();
        assert_eq!(showing.len(), 1);
        showing.remove(0)
    }

    /// Shows `notification`, and returns it as the desktop got it.
    async fn show(&self, notification: Notification) -> Posted {
        self.notifier.show(notification).await;
        self.desktop.notifications.showing().pop().unwrap()
    }

    /// The next thing a person did. It fails the test when nothing is
    /// heard in ten seconds.
    async fn event(&mut self) -> Event {
        let event = tokio::time::timeout(Duration::from_secs(10), self.events.recv());
        event.await.expect("an event is heard").unwrap()
    }

    /// Sends the signal `name` of the notification server with `body`,
    /// for a signal the server's own calls do not send.
    async fn signal<B: serde::Serialize + zbus::zvariant::DynamicType>(
        &self,
        name: &str,
        body: &B,
    ) {
        let (path, interface) = (
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        );
        let connection = &self.desktop.connection;
        let sent = connection.emit_signal(None::<&str>, path, interface, name, body);
        sent.await.unwrap();
    }

    /// What the notifier keeps of what it shows, as its file has it.
    fn kept(&self) -> HashMap<String, u32> {
        let kept = std::fs::read(self.places.state.join("notified.json")).unwrap();
        serde_json::from_slice(&kept).unwrap()
    }
}

/// A session whose desktop reports `capabilities`.
async fn session_with(capabilities: &[&str]) -> Session {
    let bus = TestBus::start();
    let desktop = TestDesktop::serve_with(bus.connect().await.unwrap(), capabilities);
    let desktop = desktop.await.unwrap();
    let places = Places::fresh();
    let (sender, events) = unbounded_channel();
    let connection = bus.connect().await.unwrap();
    let notifier = Freedesktop::open(&connection, &places.state, sender);
    Session {
        notifier: notifier.await.unwrap(),
        events,
        desktop,
        places,
        bus,
    }
}

/// A session whose desktop is GNOME Shell.
async fn session() -> Session {
    session_with(&GNOME).await
}

/// A question from `@alice` about `tag`, with a Yes button, that alerts.
fn notification(tag: &str) -> Notification {
    Notification {
        tag: tag.to_owned(),
        title: "@alice".to_owned(),
        summary: "Restart postgres?".to_owned(),
        body: "It is using 94% of its memory.".to_owned(),
        urgency: Urgency::Normal,
        alert: true,
        buttons: vec![(Action::Choice("Yes".to_owned()), "Yes".to_owned())],
        picture: None,
        icon: None,
        replaces: None,
    }
}

#[tokio::test]
async fn a_notification_is_posted_as_transoms_with_its_buttons() {
    let session = session().await;
    let posted = session.show(notification("a")).await;
    assert_eq!(posted.app_name, "transom");
    assert_eq!(posted.desktop_entry.as_deref(), Some("sh.transom"));
    assert_eq!(posted.summary, "@alice");
    assert_eq!(
        posted.body,
        "<b>Restart postgres?</b> It is using 94% of its memory."
    );
    let action = |key: &str, label: &str| (key.to_owned(), label.to_owned());
    let actions = [action("default", "Open"), action("choice:Yes", "Yes")];
    assert_eq!(posted.actions, actions);
    assert_eq!(posted.image_path, None);
}

#[tokio::test]
async fn markup_in_a_message_is_shown_as_text() {
    let session = session().await;
    let mut marked = notification("a");
    marked.summary = "a < b".to_owned();
    marked.body = "<i>R&D</i>".to_owned();
    let posted = session.show(marked).await;
    assert_eq!(posted.body, "<b>a &lt; b</b> &lt;i&gt;R&amp;D&lt;/i&gt;");
}

#[tokio::test]
async fn a_message_without_a_body_shows_its_summary_alone() {
    let session = session().await;
    let mut brief = notification("a");
    brief.body = String::new();
    assert_eq!(session.show(brief).await.body, "<b>Restart postgres?</b>");
}

#[tokio::test]
async fn a_desktop_without_markup_gets_plain_text() {
    let session = session_with(&["actions", "body"]).await;
    let posted = session.show(notification("a")).await;
    assert_eq!(
        posted.body,
        "Restart postgres?: It is using 94% of its memory."
    );
    let mut brief = notification("b");
    brief.body = String::new();
    assert_eq!(session.show(brief).await.body, "Restart postgres?");
}

#[tokio::test]
async fn a_desktop_without_actions_gets_no_buttons() {
    let session = session_with(&["body"]).await;
    let posted = session.show(notification("a")).await;
    assert_eq!(posted.actions, [("default".to_owned(), "Open".to_owned())]);
}

#[tokio::test]
async fn the_urgency_is_the_desktops_low_normal_or_critical() {
    let session = session().await;
    let cases = [
        (Urgency::VeryLow, 0),
        (Urgency::Low, 0),
        (Urgency::Normal, 1),
        (Urgency::High, 1),
        (Urgency::Urgent, 2),
    ];
    for (urgency, hint) in cases {
        let mut urgent = notification("a");
        urgent.urgency = urgency;
        assert_eq!(session.show(urgent).await.urgency, hint, "{urgency:?}");
    }
}

#[tokio::test]
async fn a_notification_that_alerts_names_a_sound() {
    let session = session().await;
    let posted = session.show(notification("a")).await;
    assert_eq!(posted.sound.as_deref(), Some("message-new-instant"));
}

#[tokio::test]
async fn a_quiet_notification_names_no_sound() {
    let session = session().await;
    let mut updated = notification("a");
    updated.alert = false;
    assert_eq!(session.show(updated).await.sound, None);
    let mut low = notification("b");
    low.urgency = Urgency::Low;
    assert_eq!(session.show(low).await.sound, None);
}

#[tokio::test]
async fn the_senders_picture_is_the_notifications_image() {
    let session = session().await;
    let mut pictured = notification("a");
    pictured.icon = Some("/cache/pictures/alice".into());
    let posted = session.show(pictured).await;
    assert_eq!(posted.image_path.as_deref(), Some("/cache/pictures/alice"));
}

#[tokio::test]
async fn showing_a_tag_again_changes_the_notification_in_place() {
    let session = session().await;
    let first = session.show(notification("a")).await;
    session.notifier.show(notification("a")).await;
    let posted = session.posted();
    assert_eq!((posted.id, posted.posts), (first.id, 2));
}

#[tokio::test]
async fn a_notification_takes_over_the_one_it_replaces() {
    let session = session().await;
    let first = session.show(notification("a")).await;
    let mut second = notification("b");
    second.replaces = Some("a".to_owned());
    session.notifier.show(second).await;
    let posted = session.posted();
    assert_eq!((posted.id, posted.posts), (first.id, 2));
    assert_eq!(session.kept(), HashMap::from([("b".to_owned(), first.id)]));
}

#[tokio::test]
async fn a_withdrawn_notification_is_taken_down() {
    let session = session().await;
    session.show(notification("a")).await;
    session.notifier.withdraw("a").await;
    session.notifier.withdraw("never shown").await;
    assert_eq!(session.desktop.notifications.showing(), []);
    assert_eq!(session.kept(), HashMap::new());
}

#[tokio::test]
async fn a_press_is_heard_once_with_its_token_and_not_as_a_dismissal() {
    let mut session = session().await;
    let pressed = session.show(notification("a")).await;
    let other = session.show(notification("b")).await;
    session
        .desktop
        .invoke(pressed.id, "choice:Yes")
        .await
        .unwrap();
    session.desktop.dismiss(other.id).await.unwrap();
    let invoked = Event::Invoked {
        tag: "a".to_owned(),
        action: Action::Choice("Yes".to_owned()),
        token: Some("test-activation-token".to_owned()),
    };
    assert_eq!(session.event().await, invoked);
    let dismissed = Event::Dismissed {
        tag: "b".to_owned(),
    };
    assert_eq!(session.event().await, dismissed);
}

#[tokio::test]
async fn a_notification_the_app_took_down_is_not_heard_as_dismissed() {
    let mut session = session().await;
    session.show(notification("a")).await;
    let other = session.show(notification("b")).await;
    session.notifier.withdraw("a").await;
    session.desktop.dismiss(other.id).await.unwrap();
    let dismissed = Event::Dismissed {
        tag: "b".to_owned(),
    };
    assert_eq!(session.event().await, dismissed);
}

#[tokio::test]
async fn a_press_in_the_moment_after_a_withdrawal_still_names_its_notification() {
    let mut session = session().await;
    let withdrawn = session.show(notification("a")).await;
    session.notifier.withdraw("a").await;
    session
        .desktop
        .invoke(withdrawn.id, "choice:Yes")
        .await
        .unwrap();
    assert!(matches!(session.event().await, Event::Invoked { tag, .. } if tag == "a"));
}

#[tokio::test]
async fn only_the_last_few_withdrawn_notifications_can_still_be_pressed() {
    let mut session = session().await;
    let mut ids = Vec::new();
    for number in 0..=WITHDRAWN_KEPT {
        let tag = number.to_string();
        ids.push(session.show(notification(&tag)).await.id);
        session.notifier.withdraw(&tag).await;
    }
    session.desktop.invoke(ids[0], "choice:Yes").await.unwrap();
    session.desktop.invoke(ids[1], "choice:Yes").await.unwrap();
    assert!(matches!(session.event().await, Event::Invoked { tag, .. } if tag == "1"));
}

#[tokio::test]
async fn a_notification_the_desktop_expired_is_forgotten_without_an_event() {
    let mut session = session().await;
    let expired = session.show(notification("a")).await;
    let other = session.show(notification("b")).await;
    session
        .signal("NotificationClosed", &(expired.id, 1_u32))
        .await;
    session.desktop.dismiss(other.id).await.unwrap();
    let dismissed = Event::Dismissed {
        tag: "b".to_owned(),
    };
    assert_eq!(session.event().await, dismissed);
    assert_eq!(session.kept(), HashMap::new());
}

#[tokio::test]
async fn a_press_takes_only_the_token_given_for_it() {
    let mut session = session().await;
    let pressed = session.show(notification("a")).await;
    session
        .signal("ActivationToken", &(pressed.id + 1, "another"))
        .await;
    session
        .signal("ActionInvoked", &(pressed.id, "reply"))
        .await;
    let invoked = Event::Invoked {
        tag: "a".to_owned(),
        action: Action::Reply,
        token: None,
    };
    assert_eq!(session.event().await, invoked);
}

#[tokio::test]
async fn what_the_app_cannot_read_or_does_not_know_is_passed_over() {
    let mut session = session().await;
    let shown = session.show(notification("a")).await;
    session.signal("ActionInvoked", &("not an id",)).await;
    session.signal("NotificationClosed", &("not an id",)).await;
    session.signal("SomethingNew", &(shown.id,)).await;
    session
        .signal("ActionInvoked", &(shown.id + 1, "reply"))
        .await;
    session
        .signal("NotificationClosed", &(shown.id + 1, 2_u32))
        .await;
    session.desktop.dismiss(shown.id).await.unwrap();
    let dismissed = Event::Dismissed {
        tag: "a".to_owned(),
    };
    assert_eq!(session.event().await, dismissed);
}

#[tokio::test]
async fn a_press_of_something_the_app_does_not_know_forgets_the_notification() {
    let mut session = session().await;
    let pressed = session.show(notification("a")).await;
    let other = session.show(notification("b")).await;
    session
        .desktop
        .invoke(pressed.id, "not an action")
        .await
        .unwrap();
    session.desktop.dismiss(other.id).await.unwrap();
    let dismissed = Event::Dismissed {
        tag: "b".to_owned(),
    };
    assert_eq!(session.event().await, dismissed);
}

#[tokio::test]
async fn what_an_earlier_run_left_showing_is_taken_down() {
    let Session {
        notifier: earlier,
        desktop,
        places,
        bus,
        ..
    } = session().await;
    earlier.show(notification("a")).await;
    let (sender, _events) = unbounded_channel();
    let connection = bus.connect().await.unwrap();
    let later = Freedesktop::open(&connection, &places.state, sender);
    later.await.unwrap();
    assert_eq!(desktop.notifications.showing(), []);
}

#[tokio::test]
async fn a_desktop_that_went_away_shows_nothing() {
    let Session {
        notifier,
        mut events,
        desktop,
        places,
        bus,
    } = session().await;
    drop((desktop, bus));
    notifier.show(notification("a")).await;
    let kept = std::fs::read_to_string(places.state.join("notified.json"));
    assert_eq!(kept.unwrap(), "{}");
    let ended = tokio::time::timeout(Duration::from_secs(10), events.recv());
    assert_eq!(ended.await.expect("the notifier stops listening"), None);
}
