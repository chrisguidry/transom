use super::*;
use crate::exchange::Link;
use crate::exchange::testing::{exchange, question};
use crate::inbox::Sending;

fn entry(exchange: Exchange) -> Entry {
    Entry {
        read: exchange.read,
        exchange,
        sending: None,
        lost: None,
    }
}

fn now() -> Timestamp {
    "2026-11-19T12:10:00Z".parse().unwrap()
}

fn since() -> Timestamp {
    "2026-11-19T11:00:00Z".parse().unwrap()
}

fn plan(posted: &mut Posted, entries: &[Entry]) -> Vec<Step> {
    posted.plan(entries, "@bob", now(), since(), false)
}

fn labels(exchange: &Exchange) -> Vec<String> {
    buttons(exchange)
        .into_iter()
        .map(|(_, label)| label)
        .collect()
}

fn link(title: &str) -> Link {
    Link {
        href: format!("https://example.com/{title}"),
        rel: None,
        title: Some(title.to_owned()),
    }
}

#[test]
fn an_unread_message_is_shown_once() {
    let mut posted = Posted::default();
    let entries = [entry(exchange("a"))];
    let steps = plan(&mut posted, &entries);
    let [Step::Show(shown)] = steps.as_slice() else {
        panic!("expected one notification, got {steps:?}");
    };
    assert_eq!(shown.tag, "a");
    assert_eq!(shown.title, "@alice");
    assert!(shown.alert);
    assert_eq!(plan(&mut posted, &entries), []);
}

#[test]
fn a_read_or_ended_exchange_is_withdrawn() {
    let mut posted = Posted::default();
    plan(&mut posted, &[entry(exchange("a")), entry(exchange("b"))]);
    let mut read = exchange("a");
    read.read = Some(true);
    let mut ended = exchange("b");
    ended.state = State::Withdrawn;
    assert_eq!(
        plan(&mut posted, &[entry(read), entry(ended)]),
        [
            Step::Withdraw("a".to_owned()),
            Step::Withdraw("b".to_owned())
        ]
    );
}

#[test]
fn an_exchange_that_left_the_inbox_is_withdrawn() {
    let mut posted = Posted::default();
    plan(&mut posted, &[entry(exchange("a"))]);
    assert_eq!(plan(&mut posted, &[]), [Step::Withdraw("a".to_owned())]);
}

#[test]
fn what_arrived_before_notifying_began_is_shown_only_if_it_is_a_question() {
    let mut old = exchange("a");
    old.created = "2026-11-19T10:00:00Z".parse().unwrap();
    let mut asked = question("b", &["Yes"]);
    asked.created = "2026-11-19T10:00:00Z".parse().unwrap();
    let steps = plan(&mut Posted::default(), &[entry(old), entry(asked)]);
    let [Step::Show(shown)] = steps.as_slice() else {
        panic!("expected one notification, got {steps:?}");
    };
    assert_eq!(shown.tag, "b");
}

#[test]
fn nothing_alerts_on_the_first_list_or_at_low_urgency() {
    let mut low = exchange("b");
    low.message.urgency = Urgency::Low;
    let mut posted = Posted::default();
    let first = posted.plan(&[entry(exchange("a"))], "@bob", now(), since(), true);
    let later = plan(&mut posted, &[entry(exchange("a")), entry(low)]);
    for step in first.iter().chain(&later) {
        let Step::Show(shown) = step else {
            panic!("expected a notification, got {step:?}");
        };
        assert!(!shown.alert);
    }
}

#[test]
fn very_low_and_past_its_deadline_are_not_shown() {
    let mut whisper = exchange("a");
    whisper.message.urgency = Urgency::VeryLow;
    let mut late = question("b", &["Yes"]);
    late.expires = Some("2026-11-19T12:05:00Z".parse().unwrap());
    let mut mine = question("c", &["Yes"]);
    mine.from = "@bob".to_owned();
    let mut queued = entry(question("d", &["Yes"]));
    queued.sending = Some(Sending {
        choice: Some("Yes".to_owned()),
        text: None,
    });
    let entries = [entry(whisper), entry(late), entry(mine), queued];
    assert_eq!(plan(&mut Posted::default(), &entries), []);
}

#[test]
fn an_urgent_repeat_alerts_again() {
    let mut urgent = exchange("a");
    urgent.message.urgency = Urgency::Urgent;
    let mut posted = Posted::default();
    plan(&mut posted, &[entry(urgent.clone())]);
    urgent.repeats = 1;
    let steps = plan(&mut posted, &[entry(urgent.clone())]);
    let [Step::Show(shown)] = steps.as_slice() else {
        panic!("expected one notification, got {steps:?}");
    };
    assert!(shown.alert);
    assert_eq!(plan(&mut posted, &[entry(urgent)]), []);
}

#[test]
fn a_replacement_takes_over_the_earlier_notification() {
    let mut posted = Posted::default();
    let mut first = exchange("a");
    first.message.key = Some("disk".to_owned());
    plan(&mut posted, &[entry(first.clone())]);
    first.state = State::Replaced;
    let mut second = exchange("b");
    second.replaces = Some("a".to_owned());
    second.created = "2026-11-19T12:05:00Z".parse().unwrap();
    let steps = plan(&mut posted, &[entry(first), entry(second)]);
    let [Step::Show(shown)] = steps.as_slice() else {
        panic!("expected one notification, got {steps:?}");
    };
    assert_eq!(shown.tag, "b");
    assert_eq!(shown.replaces.as_deref(), Some("a"));
}

/// `a`, shown and then replaced, and `b`, the message that replaced it.
fn replaced(posted: &mut Posted) -> (Exchange, Exchange) {
    let mut first = exchange("a");
    plan(posted, &[entry(first.clone())]);
    first.state = State::Replaced;
    let mut second = exchange("b");
    second.replaces = Some("a".to_owned());
    second.created = "2026-11-19T12:05:00Z".parse().unwrap();
    (first, second)
}

#[test]
fn a_replaced_message_keeps_its_notification_for_the_message_that_replaced_it() {
    let mut posted = Posted::default();
    let (first, second) = replaced(&mut posted);
    assert_eq!(plan(&mut posted, &[entry(first.clone())]), []);
    let steps = plan(&mut posted, &[entry(first), entry(second)]);
    let [Step::Show(shown)] = steps.as_slice() else {
        panic!("expected one notification, got {steps:?}");
    };
    assert_eq!(shown.replaces.as_deref(), Some("a"));
}

#[test]
fn a_replaced_message_is_withdrawn_when_its_replacement_is_not_shown() {
    let mut posted = Posted::default();
    let (first, mut second) = replaced(&mut posted);
    second.read = Some(true);
    let steps = plan(&mut posted, &[entry(first), entry(second)]);
    assert_eq!(steps, [Step::Withdraw("a".to_owned())]);
}

#[test]
fn a_dismissed_notification_is_not_shown_again() {
    let mut posted = Posted::default();
    let entries = [entry(exchange("a"))];
    plan(&mut posted, &entries);
    posted.handled("a");
    assert_eq!(plan(&mut posted, &entries), []);
    assert_eq!(posted.clear(), Vec::<String>::new());
}

#[test]
fn choices_are_buttons_when_they_all_fit() {
    assert_eq!(labels(&question("a", &["Yes", "No"])), ["Yes", "No"]);
    assert_eq!(labels(&question("a", &["1", "2", "3", "4"])), ["Answer…"]);
    assert_eq!(labels(&exchange("a")), Vec::<String>::new());
}

#[test]
fn a_prompt_gets_reply_and_the_choices_beside_it_when_they_fit() {
    let mut prompted = question("a", &["Yes", "No"]);
    prompted.message.prompt = Some("Why?".to_owned());
    assert_eq!(labels(&prompted), ["Reply…", "Yes", "No"]);
    let mut crowded = question("a", &["1", "2", "3"]);
    crowded.message.prompt = Some(String::new());
    assert_eq!(labels(&crowded), ["Reply…"]);
}

#[test]
fn an_urgent_alert_leads_with_got_it_then_its_first_link() {
    let mut alert = question("a", &["Silence"]);
    alert.message.urgency = Urgency::Urgent;
    alert.message.links = vec![link("Dashboard"), link("Runbook")];
    assert_eq!(labels(&alert), ["Got it", "Dashboard", "Silence"]);
}

#[test]
fn an_action_survives_the_trip_through_the_server() {
    let actions = [
        Action::GotIt,
        Action::Choice("restart:now".to_owned()),
        Action::Answer,
        Action::Reply,
        Action::Link("https://example.com/a?b=c".to_owned()),
        Action::Open,
    ];
    for action in actions {
        assert_eq!(Action::from_key(&action.key()), Some(action));
    }
    assert_eq!(Action::from_key("unknown"), None);
    assert_eq!(Action::from_key("other:thing"), None);
}
