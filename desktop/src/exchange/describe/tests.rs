use super::*;
use crate::exchange::testing::{exchange, question, reply};

fn entry(exchange: Exchange) -> Entry {
    Entry {
        read: exchange.read,
        exchange,
        sending: None,
        lost: None,
    }
}

#[test]
fn the_first_line_loses_its_markdown() {
    assert_eq!(first_line("**94%** full\nand rising"), "94% full");
    assert_eq!(first_line("- one\n- two"), "one");
    assert_eq!(first_line("[the run](https://ci) failed"), "the run failed");
    assert_eq!(first_line(""), "");
}

#[test]
fn a_long_title_is_cut() {
    let mut exchange = exchange("a");
    exchange.message.summary = None;
    exchange.message.body = Some("x".repeat(100));
    assert_eq!(title(&exchange).chars().count(), 81);
    assert_eq!(excerpt(&exchange), "");
}

#[test]
fn times_read_as_a_row_says_them() {
    let zone = TimeZone::UTC;
    let now: Timestamp = "2026-11-19T12:00:00Z".parse().unwrap();
    let cases = [
        ("2026-11-19T11:59:30Z", "now"),
        ("2026-11-19T11:56:00Z", "4m"),
        ("2026-11-19T09:00:00Z", "3h"),
        ("2026-11-18T23:00:00Z", "Yesterday"),
        ("2026-11-16T12:00:00Z", "Mon"),
        ("2026-11-05T12:00:00Z", "Nov 5"),
        ("2025-11-05T12:00:00Z", "Nov 5, 2025"),
    ];
    for (then, said) in cases {
        assert_eq!(ago(then.parse().unwrap(), now, &zone), said);
    }
    assert_eq!(
        in_full("2026-11-05T12:29:00Z".parse().unwrap(), &zone),
        "Thu, Nov 5 at 12:29 PM"
    );
}

#[test]
fn a_row_says_who_and_where_it_stands() {
    let mut answered = question("a", &["Restart", "Leave it"]);
    answered.state = State::Replied;
    answered.reply = Some(reply("@bob", Some("Restart"), None));
    assert_eq!(
        meta(&entry(answered), "@bob"),
        "@alice · You answered: Restart"
    );

    let mut sent = question("b", &["Yes"]);
    sent.from = "@bob".to_owned();
    sent.to = "@alice".to_owned();
    assert_eq!(
        meta(&entry(sent), "@bob"),
        "To @alice · Waiting for an answer"
    );
}

#[test]
fn an_unsent_answer_and_a_lost_one_say_so() {
    let mut waiting = entry(question("a", &["Yes", "No"]));
    waiting.sending = Some(Sending {
        choice: Some("No".to_owned()),
        text: None,
    });
    assert_eq!(
        standing(&waiting, "@bob").unwrap(),
        "Your answer, No, is not sent yet"
    );

    let mut beaten = question("a", &["Yes", "No"]);
    beaten.state = State::Replied;
    beaten.reply = Some(reply("@sam", Some("Yes"), None));
    let mut beaten = entry(beaten);
    beaten.lost = Some(Sending {
        choice: Some("No".to_owned()),
        text: None,
    });
    assert_eq!(
        standing(&beaten, "@bob").unwrap(),
        "Your answer, No, was not used: @sam answered first"
    );
}

#[test]
fn the_first_line_keeps_html_as_text_and_passes_a_paragraph_without_words() {
    assert_eq!(first_line("<hr>"), "<hr>");
    assert_eq!(first_line("<b>Disk</b> full"), "<b>Disk</b> full");
    assert_eq!(first_line("![](graph.png)\n\nDisk full"), "Disk full");
}

#[test]
fn a_party_is_its_name_with_its_handle_or_its_handle_alone() {
    assert_eq!(party("@alice", Some("Alice")), "Alice (@alice)");
    assert_eq!(party("@alice", None), "@alice");
}

/// A notification from `@alice` in `state`.
fn in_state(state: State) -> Entry {
    let mut exchange = exchange("a");
    exchange.state = state;
    entry(exchange)
}

#[test]
fn an_ended_exchange_says_how_it_ended() {
    let cases = [
        (State::Pending, "Waiting to be accepted"),
        (State::Replied, "Answered"),
        (State::Expired, "Expired"),
        (State::Withdrawn, "Withdrawn"),
        (State::Declined, "Declined"),
        (State::Replaced, "Replaced by a newer message"),
    ];
    for (state, said) in cases {
        assert_eq!(standing(&in_state(state), "@bob").as_deref(), Some(said));
    }
}

#[test]
fn an_open_exchange_from_someone_else_says_nothing() {
    assert_eq!(standing(&in_state(State::Open), "@bob"), None);
}

#[test]
fn a_question_that_expired_says_it_went_unanswered() {
    let mut expired = question("a", &["Yes"]);
    expired.state = State::Expired;
    let said = standing(&entry(expired), "@bob");
    assert_eq!(said.as_deref(), Some("Expired unanswered"));
}

#[test]
fn someone_elses_answer_is_said_by_their_name() {
    let mut answered = question("a", &["Yes", "No"]);
    answered.state = State::Replied;
    let mut by_sam = reply("@sam", None, Some("Not today"));
    by_sam.by_name = Some("Sam".to_owned());
    answered.reply = Some(by_sam);
    let said = standing(&entry(answered), "@bob");
    assert_eq!(said.as_deref(), Some("Sam answered: Not today"));
}

#[test]
fn a_row_for_a_groups_message_says_who_it_was_to() {
    let mut to_team = exchange("a");
    to_team.to = "@team".to_owned();
    assert_eq!(meta(&entry(to_team.clone()), "@bob"), "@alice · to @team");
    to_team.to_name = Some("The team".to_owned());
    assert_eq!(meta(&entry(to_team), "@bob"), "@alice · to The team");
}

#[test]
fn a_page_says_only_that_a_replied_exchange_was_answered() {
    let mut answered = question("a", &["Yes", "No"]);
    answered.state = State::Replied;
    answered.reply = Some(reply("@bob", Some("Yes"), None));
    let said = standing_on_page(&entry(answered));
    assert_eq!(said.as_deref(), Some("Answered"));
}

#[test]
fn a_page_says_nothing_of_an_answer_that_is_not_sent_yet() {
    let mut waiting = entry(question("a", &["Yes", "No"]));
    waiting.sending = Some(Sending {
        choice: Some("No".to_owned()),
        text: None,
    });
    assert_eq!(standing_on_page(&waiting), None);
}

#[test]
fn the_line_over_a_page_names_both_parties_the_time_and_where_it_stands() {
    let mut open = exchange("a");
    open.from_name = Some("Alice".to_owned());
    let line = page_meta(&entry(open.clone()), &TimeZone::UTC);
    assert_eq!(line, "Alice (@alice) to @bob · Thu, Nov 19 at 12:00 PM");
    open.state = State::Withdrawn;
    let line = page_meta(&entry(open), &TimeZone::UTC);
    assert!(line.ends_with("12:00 PM · Withdrawn"));
}
