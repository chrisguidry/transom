//! The inbox as this device has it: the server's last description of
//! each exchange, with what the outbox holds for it applied on top, sorted
//! into the sections the web's inbox shows.

pub mod screen;

use std::collections::{HashMap, HashSet};

use jiff::{SignedDuration, Timestamp};

use crate::account::Snapshot;
use crate::exchange::{Choice, Exchange, State, TRANSOM};

/// How many rows Earlier shows before the rest are Older.
const EARLIER_ROWS: usize = 50;

/// A reply this device made and the server has not taken yet, or took
/// too late.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Sending {
    pub choice: Option<String>,
    pub text: Option<String>,
}

/// One exchange in the inbox. `read` is `None` when this person is not
/// one of its recipients. `sending` is a reply the outbox holds, which
/// the exchange shows in place of its own until the server settles it.
/// `lost` is a reply of this device's that came after someone else's.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub exchange: Exchange,
    pub read: Option<bool>,
    pub sending: Option<Sending>,
    pub lost: Option<Sending>,
}

impl Entry {
    /// Whether this waits on `me`: an open question someone else asked,
    /// which asks for an answer and which `me` has not answered. A
    /// question with choices and links, and no prompt, is an alert whose
    /// choices are actions, such as Silence beside a dashboard: nothing
    /// breaks if no one takes them. The web's `inbox::waits_on` is the
    /// same rule.
    pub fn waits_on(&self, me: &str) -> bool {
        let exchange = &self.exchange;
        exchange.question
            && exchange.state == State::Open
            && exchange.from != me
            && self.sending.is_none()
            && (exchange.message.prompt.is_some() || exchange.message.links.is_empty())
    }

    /// The choices `me` may answer with from a row, without opening the
    /// exchange: those of an open question to them, three at most, when
    /// no prompt asks for text.
    pub fn row_choices(&self, me: &str) -> &[Choice] {
        let exchange = &self.exchange;
        let answerable = exchange.state == State::Open
            && exchange.from != me
            && self.read.is_some()
            && self.sending.is_none()
            && exchange.message.prompt.is_none()
            && exchange.message.choices.len() <= 3;
        if answerable {
            &exchange.message.choices
        } else {
            &[]
        }
    }

    /// Whether `me` may reply now.
    pub fn can_reply(&self, me: &str) -> bool {
        self.exchange.state == State::Open
            && self.exchange.from != me
            && self.read.is_some()
            && self.sending.is_none()
    }

    /// Whether `me` may withdraw the message: its sender, while it is
    /// open or held.
    pub fn can_withdraw(&self, me: &str) -> bool {
        self.exchange.from == me && matches!(self.exchange.state, State::Open | State::Pending)
    }
}

/// One row of the inbox: the newest of `entries`, which repeat one alert
/// under one key.
#[derive(Clone, Debug, PartialEq)]
pub struct Listing {
    pub entries: Vec<Entry>,
}

impl Listing {
    pub fn entry(&self) -> &Entry {
        &self.entries[0]
    }

    pub fn ids(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.exchange.id.clone())
            .collect()
    }
}

/// `me`'s inbox, sorted as the web's inbox sorts it. `waiting` holds the
/// open questions that wait on this person, `unread` the rest of what
/// they have not read, and `earlier` everything else from the last week,
/// 50 rows at most, with `older` past that. Each is newest first.
///
/// The sections leave out each exchange that a later one with the same
/// key replaced, so a firing alert and its resolution are one row, and
/// each request from `@transom` that was answered or has an answer
/// queued, since its outcome is the sender's own message.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inbox {
    pub waiting: Vec<Entry>,
    pub unread: Vec<Listing>,
    pub earlier: Vec<Listing>,
    pub older: Vec<Listing>,
    /// What Mark all read marks: each exchange in `unread`, and each
    /// replaced one this person never read, which the inbox no longer
    /// lists.
    pub markable: Vec<String>,
}

impl Inbox {
    /// The sections of `entries` for `me`, a handle with its `@`, at
    /// `now`.
    pub fn of(me: &str, entries: &[Entry], now: Timestamp) -> Self {
        let mut entries: Vec<&Entry> = entries.iter().collect();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.exchange.created));
        let replaced: HashSet<&str> = entries
            .iter()
            .filter_map(|entry| entry.exchange.replaces.as_deref())
            .collect();
        let is_replaced = |entry: &Entry| {
            entry.exchange.state == State::Replaced || replaced.contains(entry.exchange.id.as_str())
        };
        let is_hidden = |entry: &Entry| {
            is_replaced(entry)
                || (entry.exchange.from == TRANSOM
                    && (entry.exchange.state != State::Open || entry.sending.is_some()))
        };
        let listed: Vec<&Entry> = entries
            .iter()
            .copied()
            .filter(|entry| !is_hidden(entry))
            .collect();
        let waiting: Vec<Entry> = listed
            .iter()
            .filter(|entry| entry.waits_on(me))
            .map(|entry| (*entry).clone())
            .collect();
        let rest = |unread: bool| {
            collapse(
                listed
                    .iter()
                    .filter(|entry| !entry.waits_on(me) && (entry.read == Some(false)) == unread),
            )
        };
        let unread = rest(true);
        let mut earlier = rest(false);
        let week_ago = now - SignedDuration::from_hours(7 * 24);
        let shown = earlier
            .iter()
            .take(EARLIER_ROWS)
            .take_while(|listing| listing.entry().exchange.created >= week_ago)
            .count();
        let older = earlier.split_off(shown);
        let markable = unread
            .iter()
            .flat_map(Listing::ids)
            .chain(
                entries
                    .iter()
                    .filter(|entry| is_replaced(entry) && entry.read == Some(false))
                    .map(|entry| entry.exchange.id.clone()),
            )
            .collect();
        Self {
            waiting,
            unread,
            earlier,
            older,
            markable,
        }
    }

    /// Whether there is nothing in the inbox at all.
    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
            && self.unread.is_empty()
            && self.earlier.is_empty()
            && self.older.is_empty()
    }
}

/// What the inbox's page draws from. The page draws again only when
/// this differs from what it last drew, so a snapshot that changes
/// nothing it shows draws nothing.
#[derive(Debug, PartialEq)]
pub struct Drawn {
    pub me: String,
    pub inbox: Inbox,
    pub loaded: bool,
    pub connected: bool,
    /// The minute it was drawn in, since each row says how long ago its
    /// exchange arrived.
    minute: i64,
}

impl Drawn {
    /// What the page draws of `snapshot` for `me` at `now`.
    pub fn of(me: &str, snapshot: &Snapshot, now: Timestamp) -> Self {
        Self {
            me: me.to_owned(),
            inbox: Inbox::of(me, &snapshot.entries, now),
            loaded: snapshot.loaded,
            connected: snapshot.connected,
            minute: now.as_second() / 60,
        }
    }
}

/// `entries` as rows, the exchanges one sender sent one recipient with
/// the same key as one, in the order their newest arrived.
fn collapse<'a>(entries: impl Iterator<Item = &'a &'a Entry>) -> Vec<Listing> {
    let mut rows: Vec<Listing> = Vec::new();
    let mut keyed: HashMap<(String, String, String), usize> = HashMap::new();
    for entry in entries {
        let exchange = &entry.exchange;
        let Some(key) = exchange.message.key.clone() else {
            rows.push(Listing {
                entries: vec![(*entry).clone()],
            });
            continue;
        };
        let key = (exchange.from.clone(), exchange.to.clone(), key);
        if let Some(&row) = keyed.get(&key) {
            rows[row].entries.push((*entry).clone());
        } else {
            keyed.insert(key, rows.len());
            rows.push(Listing {
                entries: vec![(*entry).clone()],
            });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchange::testing::{exchange, question};

    fn entry(exchange: Exchange) -> Entry {
        Entry {
            read: exchange.read,
            exchange,
            sending: None,
            lost: None,
        }
    }

    fn now() -> Timestamp {
        "2026-11-19T13:00:00Z".parse().unwrap()
    }

    #[test]
    fn an_open_question_waits_and_an_unread_notification_is_unread() {
        let mut read = exchange("c");
        read.read = Some(true);
        let entries = [
            entry(question("a", &["Yes", "No"])),
            entry(exchange("b")),
            entry(read),
        ];
        let inbox = Inbox::of("@bob", &entries, now());
        assert_eq!(inbox.waiting[0].exchange.id, "a");
        assert_eq!(inbox.unread[0].entry().exchange.id, "b");
        assert_eq!(inbox.earlier[0].entry().exchange.id, "c");
        assert_eq!(inbox.markable, ["b"]);
        assert!(!inbox.is_empty());
    }

    #[test]
    fn a_queued_answer_stops_a_question_waiting() {
        let mut answered = entry(question("a", &["Yes", "No"]));
        answered.sending = Some(Sending {
            choice: Some("Yes".to_owned()),
            text: None,
        });
        let inbox = Inbox::of("@bob", &[answered], now());
        assert_eq!(inbox.waiting, Vec::<Entry>::new());
        assert_eq!(inbox.unread.len(), 1);
    }

    #[test]
    fn exchanges_with_one_key_are_one_row_and_a_replaced_one_is_left_out() {
        let mut first = exchange("a");
        first.message.key = Some("disk".to_owned());
        first.state = State::Replaced;
        let mut second = exchange("b");
        second.message.key = Some("disk".to_owned());
        second.replaces = Some("a".to_owned());
        second.created = "2026-11-19T12:30:00Z".parse().unwrap();
        let mut third = exchange("c");
        third.message.key = Some("disk".to_owned());
        third.created = "2026-11-19T12:45:00Z".parse().unwrap();
        let inbox = Inbox::of("@bob", &[entry(first), entry(second), entry(third)], now());
        assert_eq!(inbox.unread.len(), 1);
        assert_eq!(inbox.unread[0].ids(), ["c", "b"]);
        assert_eq!(inbox.markable, ["c", "b", "a"]);
    }

    #[test]
    fn a_settled_request_from_transom_is_left_out() {
        let mut request = question("a", &["Allow", "Decline"]);
        request.from = TRANSOM.to_owned();
        request.state = State::Replied;
        assert!(Inbox::of("@bob", &[entry(request)], now()).is_empty());
    }

    #[test]
    fn what_is_older_than_a_week_is_older() {
        let mut old = exchange("a");
        old.read = Some(true);
        old.created = "2026-11-01T12:00:00Z".parse().unwrap();
        let inbox = Inbox::of("@bob", &[entry(old)], now());
        assert_eq!(inbox.earlier, Vec::<Listing>::new());
        assert_eq!(inbox.older.len(), 1);
    }

    #[test]
    fn a_recipient_may_reply_to_an_open_exchange_once() {
        let mut asked = entry(question("a", &["Yes", "No"]));
        assert!(asked.can_reply("@bob"));
        assert!(!asked.can_reply("@alice"));
        asked.sending = Some(Sending {
            choice: Some("Yes".to_owned()),
            text: None,
        });
        assert!(!asked.can_reply("@bob"));
    }

    #[test]
    fn someone_who_is_no_recipient_or_too_late_may_not_reply() {
        let mut watched = entry(question("a", &["Yes", "No"]));
        watched.read = None;
        assert!(!watched.can_reply("@carol"));
        let mut ended = question("b", &["Yes", "No"]);
        ended.state = State::Expired;
        assert!(!entry(ended).can_reply("@bob"));
    }

    #[test]
    fn only_the_sender_may_withdraw_and_only_while_it_is_open_or_held() {
        let mut sent = question("a", &["Yes", "No"]);
        assert!(entry(sent.clone()).can_withdraw("@alice"));
        assert!(!entry(sent.clone()).can_withdraw("@bob"));
        sent.state = State::Pending;
        assert!(entry(sent.clone()).can_withdraw("@alice"));
        sent.state = State::Replied;
        assert!(!entry(sent).can_withdraw("@alice"));
    }

    /// A snapshot of `@bob`'s inbox with `entries`, loaded and connected.
    fn snapshot(entries: Vec<Entry>) -> Snapshot {
        Snapshot {
            entries,
            loaded: true,
            connected: true,
            ..Snapshot::default()
        }
    }

    #[test]
    fn the_page_draws_the_inbox_and_the_connection() {
        let drawn = Drawn::of("@bob", &snapshot(vec![entry(exchange("a"))]), now());
        assert_eq!(drawn.me, "@bob");
        assert_eq!(drawn.inbox.unread.len(), 1);
        assert!(drawn.loaded && drawn.connected);
    }

    #[test]
    fn the_page_draws_again_when_the_inbox_or_the_minute_changes() {
        let first = snapshot(vec![entry(exchange("a"))]);
        let later = now() + SignedDuration::from_secs(60);
        assert_eq!(
            Drawn::of("@bob", &first, now()),
            Drawn::of("@bob", &first, now())
        );
        assert_ne!(
            Drawn::of("@bob", &first, now()),
            Drawn::of("@bob", &first, later)
        );
        let second = snapshot(vec![entry(exchange("a")), entry(exchange("b"))]);
        assert_ne!(
            Drawn::of("@bob", &first, now()),
            Drawn::of("@bob", &second, now())
        );
    }

    #[test]
    fn a_row_offers_up_to_three_choices_without_a_prompt() {
        let few = entry(question("a", &["Yes", "No"]));
        assert_eq!(few.row_choices("@bob").len(), 2);
        let many = entry(question("b", &["1", "2", "3", "4"]));
        assert_eq!(many.row_choices("@bob"), []);
        let mut prompted = question("c", &["Yes"]);
        prompted.message.prompt = Some("Why?".to_owned());
        assert_eq!(entry(prompted).row_choices("@bob"), []);
        assert_eq!(few.row_choices("@alice"), []);
    }
}
