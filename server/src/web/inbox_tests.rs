//! Which exchanges the inbox lists, under which section, and what each row
//! says, from the JSON view of each exchange.

use jiff::Timestamp;
use serde_json::{Value, json};

use super::inbox::{inbox, meta, relative, waits_on};

const ME: &str = "@chris";

fn now() -> Timestamp {
    "2026-11-05T12:00:00Z".parse().unwrap()
}

/// An open one-way message from Alertmanager to `@chris`, unread, made
/// `minutes` before `now`, with `changes` laid over it: a top-level field
/// replaces the exchange's own, and `message` replaces fields of the
/// message.
fn exchange(id: &str, minutes: i64, changes: &Value) -> Value {
    let created = now() - jiff::Span::new().minutes(minutes);
    let mut exchange = json!({
        "id": id, "state": "open", "question": false,
        "from": "@alertmanager", "from_name": "Alertmanager", "to": ME,
        "created": created.to_string(),
        "message": {"summary": id, "urgency": "normal"}, "read": false,
    });
    for (field, value) in changes.as_object().unwrap() {
        if field == "message" {
            for (field, value) in value.as_object().unwrap() {
                exchange["message"][field] = value.clone();
            }
        } else {
            exchange[field] = value.clone();
        }
    }
    exchange
}

fn ids(rows: &[super::inbox::Row]) -> Vec<&str> {
    rows.iter().map(|row| row.id.as_str()).collect()
}

#[test]
fn a_question_waits_when_it_asks_for_an_answer() {
    let choices = json!([{"label": "Restart"}]);
    let link = json!([{"href": "https://grafana/d"}]);
    let cases = [
        (
            json!({"question": true, "message": {"choices": choices}}),
            true,
        ),
        (
            json!({"question": true, "message": {"prompt": "Why?", "link": link}}),
            true,
        ),
        (
            json!({"question": true, "message": {"choices": choices, "link": link}}),
            false,
        ),
        (json!({"question": true, "from": ME}), false),
        (json!({"question": true, "state": "replied"}), false),
        (json!({}), false),
    ];
    for (changes, waits) in cases {
        assert_eq!(
            waits_on(&exchange("x", 1, &changes), ME),
            waits,
            "{changes}"
        );
    }
}

#[test]
fn sorts_what_waits_what_is_unread_and_what_came_earlier() {
    let listed = [
        exchange("asked", 1, &json!({"question": true, "read": true})),
        exchange("new", 2, &json!({})),
        exchange("seen", 3, &json!({"read": true})),
        exchange(
            "sent",
            4,
            &json!({"from": ME, "to": "@claude", "read": null}),
        ),
    ];
    let inbox = inbox(&listed, ME, now());
    assert_eq!(
        (ids(&inbox.waiting), ids(&inbox.unread), ids(&inbox.earlier)),
        (vec!["asked"], vec!["new"], vec!["seen", "sent"])
    );
}

#[test]
fn leaves_out_what_was_replaced_and_answered_requests() {
    let listed = [
        exchange("resolved", 1, &json!({"replaces": "firing"})),
        exchange("firing", 2, &json!({})),
        exchange("older", 3, &json!({"state": "replaced"})),
        exchange(
            "allowed",
            4,
            &json!({"from": "@transom", "state": "replied"}),
        ),
        exchange("asking", 5, &json!({"from": "@transom", "question": true})),
    ];
    let inbox = inbox(&listed, ME, now());
    assert_eq!(
        (ids(&inbox.waiting), ids(&inbox.unread)),
        (vec!["asking"], vec!["resolved"])
    );
}

#[test]
fn one_alert_repeated_with_its_key_is_one_row() {
    let keyed = json!({"read": true, "message": {"key": "disk"}});
    let listed = [
        exchange("third", 1, &keyed),
        exchange("other", 2, &json!({"read": true})),
        exchange("second", 3, &keyed),
        exchange("first", 4, &keyed),
    ];
    let inbox = inbox(&listed, ME, now());
    let row = &inbox.earlier[0];
    assert_eq!(
        (ids(&inbox.earlier), row.ids.len(), row.meta.as_str()),
        (vec!["third", "other"], 3, "Alertmanager · 3 times")
    );
}

#[test]
fn mark_all_read_marks_every_exchange_a_row_stands_for() {
    let keyed = json!({"message": {"key": "disk"}});
    let listed = [exchange("second", 1, &keyed), exchange("first", 2, &keyed)];
    assert_eq!(inbox(&listed, ME, now()).unread_ids(), ["second", "first"]);
}

#[test]
fn earlier_shows_the_last_week_and_fifty_rows_at_most() {
    let mut listed: Vec<Value> = (0..55)
        .map(|n| exchange(&format!("recent-{n:02}"), n, &json!({"read": true})))
        .collect();
    listed.push(exchange("old", 8 * 24 * 60, &json!({"read": true})));
    let inbox = inbox(&listed, ME, now());
    assert_eq!(
        (
            inbox.earlier.len(),
            inbox.older.len(),
            inbox.older[5].id.as_str()
        ),
        (50, 6, "old")
    );
}

#[test]
fn an_inbox_with_nothing_in_it_is_empty() {
    assert!(inbox(&[], ME, now()).is_empty());
    assert!(!inbox(&[exchange("new", 1, &json!({}))], ME, now()).is_empty());
}

#[test]
fn an_unread_row_is_full_size_with_its_urgencys_dot() {
    let cases = [
        ("urgent", Some("fire")),
        ("high", Some("fire")),
        ("normal", Some("warn")),
        ("low", None),
    ];
    for (urgency, dot) in cases {
        let listed = [exchange(
            "new",
            1,
            &json!({"message": {"urgency": urgency}}),
        )];
        let row = &inbox(&listed, ME, now()).unread[0];
        assert_eq!((row.full, row.dot), (true, dot), "{urgency}");
    }
}

#[test]
fn a_waiting_row_stays_full_size_once_read_but_loses_its_dot() {
    let listed = [exchange(
        "asked",
        1,
        &json!({"question": true, "read": true}),
    )];
    let row = &inbox(&listed, ME, now()).waiting[0];
    assert_eq!((row.full, row.unread, row.dot), (true, false, None));
}

#[test]
fn a_read_row_is_quiet() {
    let listed = [exchange("seen", 1, &json!({"read": true}))];
    let row = &inbox(&listed, ME, now()).earlier[0];
    assert_eq!((row.full, row.dot, row.choices.len()), (false, None, 0));
}

#[test]
fn a_row_answers_a_short_question_with_its_choices() {
    let choices = json!([{"label": "Raise and restart", "id": "raise"}, {"label": "Leave it"}]);
    let listed = [exchange(
        "asked",
        1,
        &json!({"question": true, "message": {"choices": choices}}),
    )];
    let row = &inbox(&listed, ME, now()).waiting[0];
    assert_eq!(
        row.choices,
        [
            ("raise".to_owned(), "Raise and restart".to_owned()),
            ("Leave it".to_owned(), "Leave it".to_owned())
        ]
    );
}

#[test]
fn a_row_leaves_to_the_page_what_it_cannot_answer_in_one_tap() {
    let four = json!([{"label": "A"}, {"label": "B"}, {"label": "C"}, {"label": "D"}]);
    let cases = [
        json!({"question": true, "message": {"choices": four}}),
        json!({"question": true, "message": {"choices": [{"label": "A"}], "prompt": "Why?"}}),
    ];
    for changes in cases {
        let listed = [exchange("asked", 1, &changes)];
        assert!(
            inbox(&listed, ME, now()).waiting[0].choices.is_empty(),
            "{changes}"
        );
    }
}

#[test]
fn an_excerpt_is_the_bodys_first_line_without_its_markdown() {
    let long = "word ".repeat(30);
    let cases = [
        (
            json!({"body": "A pod in **paperless/web** restarted.\nMore."}),
            "A pod in paperless/web restarted.",
        ),
        (json!({"body": long}), &*format!("{}…", &long[..120])),
        (json!({"summary": null, "body": "The title already"}), ""),
    ];
    for (message, excerpt) in cases {
        let listed = [exchange("new", 1, &json!({"message": message}))];
        assert_eq!(
            inbox(&listed, ME, now()).unread[0].excerpt,
            excerpt,
            "{message}"
        );
    }
}

#[test]
fn a_meta_line_says_who_and_where_it_stands_in_words() {
    let choices = json!([{"label": "Raise and restart", "id": "raise"}]);
    let cases = [
        (json!({}), "Alertmanager"),
        (
            json!({"to": "@ops", "to_name": "Ops"}),
            "Alertmanager · to Ops",
        ),
        (
            json!({"question": true, "state": "replied", "message": {"choices": choices}, "reply": {"by": ME, "choice": "raise"}}),
            "Alertmanager · You answered: Raise and restart",
        ),
        (
            json!({"question": true, "state": "replied", "reply": {"by": "@anna", "by_name": "Anna", "choice": "gone"}}),
            "Alertmanager · Anna answered: gone",
        ),
        (
            json!({"question": true, "state": "replied", "reply": {"by": "@bob", "text": "on it"}}),
            "Alertmanager · @bob answered: on it",
        ),
        (
            json!({"question": true, "state": "expired"}),
            "Alertmanager · Expired unanswered",
        ),
        (json!({"state": "expired"}), "Alertmanager · Expired"),
        (json!({"state": "withdrawn"}), "Alertmanager · Withdrawn"),
        (json!({"state": "declined"}), "Alertmanager · Declined"),
        (
            json!({"from": ME, "to": "@claude", "to_name": "Claude", "question": true}),
            "To Claude · Waiting for an answer",
        ),
        (
            json!({"from": ME, "to": "@claude", "state": "pending"}),
            "To @claude · Waiting to be accepted",
        ),
        (json!({"from": ME, "to": "@claude"}), "To @claude"),
    ];
    for (changes, line) in cases {
        assert_eq!(meta(&exchange("x", 1, &changes), ME), line, "{changes}");
    }
}

#[test]
fn a_time_says_how_long_ago_it_was() {
    let cases = [
        ("2026-11-05T11:59:30Z", "now"),
        ("2026-11-05T11:56:00Z", "4m"),
        ("2026-11-05T09:00:00Z", "3h"),
        ("2026-11-04T20:00:00Z", "Yesterday"),
        ("2026-11-03T08:00:00Z", "Tue"),
        ("2026-10-02T08:00:00Z", "Oct 2"),
        ("2025-11-02T08:00:00Z", "Nov 2, 2025"),
    ];
    for (at, said) in cases {
        assert_eq!(relative(at.parse().unwrap(), now()), said, "{at}");
    }
}

#[test]
fn a_row_whose_time_cannot_be_read_shows_it_as_it_came() {
    let listed = [exchange("new", 1, &json!({"created": "soon"}))];
    assert_eq!(inbox(&listed, ME, now()).unread[0].when.relative, "soon");
}

#[test]
fn a_page_says_its_time_in_full() {
    let cases = [
        (json!("2026-11-05T17:29:00Z"), "Thu, Nov 5 at 17:29 UTC"),
        (json!("soon"), "soon"),
    ];
    for (at, said) in cases {
        assert_eq!(super::pages::when(&at).text, said, "{at}");
    }
}

#[test]
fn a_page_says_where_an_exchange_stands_in_words() {
    let cases = [
        ("open", true, None),
        ("pending", false, Some("Waiting to be accepted")),
        ("replied", true, Some("Answered")),
        ("expired", true, Some("Expired unanswered")),
        ("expired", false, Some("Expired")),
        ("withdrawn", true, Some("Withdrawn")),
        ("declined", false, Some("Declined")),
        ("replaced", false, Some("Replaced by a newer message")),
    ];
    for (state, question, words) in cases {
        assert_eq!(super::pages::standing(state, question), words, "{state}");
    }
}
