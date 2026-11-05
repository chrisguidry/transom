//! The inbox page's model: which exchanges it lists, under which section,
//! and what each row says. It works on the same JSON view of an exchange
//! that the API returns, and the Android app follows the same rules, which
//! plan 18 records.

use std::collections::{HashMap, HashSet};

use jiff::{Timestamp, ToSpan, tz::TimeZone};
use serde_json::Value;

use super::markdown;
use super::pages::{Party, text, title};

/// How many rows Earlier shows before "Show older", and how far back.
const EARLIER_ROWS: usize = 50;
const EARLIER_HOURS: i64 = 7 * 24;

/// The longest excerpt a row shows, in characters.
const EXCERPT: usize = 120;

/// One row of the inbox.
pub(crate) struct Row {
    pub(crate) id: String,
    /// Every exchange the row stands for: itself, and the older ones with
    /// the same key that it collapses.
    pub(crate) ids: Vec<String>,
    pub(crate) title: String,
    pub(crate) excerpt: String,
    pub(crate) pictured: Party,
    pub(crate) meta: String,
    pub(crate) when: When,
    pub(crate) unread: bool,
    /// Whether the row is drawn at full size: unread, or waiting on the
    /// viewer however it was read.
    pub(crate) full: bool,
    /// The urgency's dot on the picture, `fire` or `warn`, while unread.
    pub(crate) dot: Option<&'static str>,
    /// The choices the row answers with, each its key and label.
    pub(crate) choices: Vec<(String, String)>,
}

/// When an exchange arrived: the RFC 3339 time, and how long ago, which
/// the page's script says again in the browser's own time zone.
pub(crate) struct When {
    pub(crate) iso: String,
    pub(crate) relative: String,
}

/// The inbox's sections, newest first in each.
pub(crate) struct Inbox {
    pub(crate) waiting: Vec<Row>,
    pub(crate) unread: Vec<Row>,
    pub(crate) earlier: Vec<Row>,
    /// Earlier rows past the last week or the first 50, behind "Show older".
    pub(crate) older: Vec<Row>,
}

impl Inbox {
    /// Every exchange the unread section stands for, which Mark all read
    /// marks.
    pub(crate) fn unread_ids(&self) -> Vec<&str> {
        self.unread
            .iter()
            .flat_map(|row| row.ids.iter().map(String::as_str))
            .collect()
    }

    /// Whether the inbox has nothing to show at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.waiting.is_empty() && self.unread.is_empty() && self.earlier.is_empty()
    }
}

/// Whether `exchange` waits on `me`: an open question someone else asked,
/// which asks for an answer rather than offering actions. A question with
/// a prompt asks for one, and so does one with only choices. One with
/// choices and links, and no prompt, is an alert whose choices are
/// actions, such as Silence beside a dashboard: nothing breaks if no one
/// takes them.
pub(crate) fn waits_on(exchange: &Value, me: &str) -> bool {
    let message = &exchange["message"];
    let linked = message["link"]
        .as_array()
        .is_some_and(|links| !links.is_empty());
    exchange["question"] == true
        && exchange["state"] == "open"
        && exchange["from"] != me
        && (message["prompt"].is_string() || !linked)
}

/// Whether the inbox leaves `exchange` out: one that a later message with
/// the same key replaced, and a request from transom that was answered,
/// such as a sender's consent request, since its outcome is the sender's
/// own message in the list.
fn hidden(exchange: &Value, replaced: &HashSet<&str>) -> bool {
    exchange["state"] == "replaced"
        || replaced.contains(exchange["id"].as_str().unwrap_or_default())
        || (exchange["from"] == "@transom" && exchange["state"] != "open")
}

/// `exchanges`, newest first, sorted into the sections `me` sees `now`.
pub(crate) fn inbox(exchanges: &[Value], me: &str, now: Timestamp) -> Inbox {
    let replaced: HashSet<&str> = exchanges
        .iter()
        .filter_map(|exchange| exchange["replaces"].as_str())
        .collect();
    let listed: Vec<&Value> = exchanges
        .iter()
        .filter(|exchange| !hidden(exchange, &replaced))
        .collect();
    let (waiting, rest): (Vec<&Value>, Vec<&Value>) = listed
        .into_iter()
        .partition(|exchange| waits_on(exchange, me));
    let (unread, read): (Vec<&Value>, Vec<&Value>) = rest
        .into_iter()
        .partition(|exchange| exchange["read"] == false);
    let week_ago = now - EARLIER_HOURS.hours();
    let mut earlier = collapse(&read, me, now);
    let shown = earlier
        .iter()
        .take(EARLIER_ROWS)
        .take_while(|row| {
            row.when
                .iso
                .parse::<Timestamp>()
                .is_ok_and(|at| at >= week_ago)
        })
        .count();
    let older = earlier.split_off(shown);
    Inbox {
        waiting: waiting
            .into_iter()
            .map(|exchange| row(&[exchange], me, now))
            .collect(),
        unread: collapse(&unread, me, now),
        earlier,
        older,
    }
}

/// One row for each exchange, except that the exchanges one sender sent
/// one recipient with the same key, which repeat one alert, are one row:
/// the newest, saying how many times it came.
fn collapse(exchanges: &[&Value], me: &str, now: Timestamp) -> Vec<Row> {
    let mut groups: Vec<Vec<&Value>> = Vec::new();
    let mut keyed: HashMap<(String, String, String), usize> = HashMap::new();
    for &exchange in exchanges {
        let Some(key) = exchange["message"]["key"].as_str() else {
            groups.push(vec![exchange]);
            continue;
        };
        let key = (
            text(&exchange["from"]),
            text(&exchange["to"]),
            key.to_owned(),
        );
        if let Some(&group) = keyed.get(&key) {
            groups[group].push(exchange);
        } else {
            keyed.insert(key, groups.len());
            groups.push(vec![exchange]);
        }
    }
    groups.iter().map(|group| row(group, me, now)).collect()
}

/// The row of `group`, the newest exchange first and the older ones with
/// its key after it.
fn row(group: &[&Value], me: &str, now: Timestamp) -> Row {
    let exchange = group[0];
    let message = &exchange["message"];
    let unread = exchange["read"] == false;
    let waiting = waits_on(exchange, me);
    let other = if exchange["from"] == me { "to" } else { "from" };
    let mut meta = meta(exchange, me);
    if group.len() > 1 {
        meta = format!("{meta} · {} times", group.len());
    }
    let created = text(&exchange["created"]);
    let relative = created
        .parse::<Timestamp>()
        .map_or_else(|_| created.clone(), |at| super::inbox::relative(at, now));
    Row {
        id: text(&exchange["id"]),
        ids: group.iter().map(|exchange| text(&exchange["id"])).collect(),
        title: title(message),
        excerpt: excerpt(message),
        pictured: Party::of(exchange, other),
        meta,
        when: When {
            iso: created,
            relative,
        },
        unread,
        full: unread || waiting,
        dot: if unread { dot(message) } else { None },
        choices: if unread || waiting {
            choices(exchange, me)
        } else {
            Vec::new()
        },
    }
}

/// The dot an unread row's picture carries for its urgency: red for
/// urgent and high, amber for normal, and none below.
fn dot(message: &Value) -> Option<&'static str> {
    match message["urgency"].as_str().unwrap_or("normal") {
        "urgent" | "high" => Some("fire"),
        "normal" => Some("warn"),
        _ => None,
    }
}

/// The choices a row offers as buttons: those of an open question `me`
/// may answer, when there are three at most and no prompt asks for text.
/// Anything else is answered on the exchange's page.
fn choices(exchange: &Value, me: &str) -> Vec<(String, String)> {
    let message = &exchange["message"];
    let choices = message["choices"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let answerable = exchange["state"] == "open"
        && exchange["from"] != me
        && exchange["read"].is_boolean()
        && !message["prompt"].is_string()
        && choices.len() <= 3;
    if !answerable {
        return Vec::new();
    }
    choices
        .iter()
        .map(|choice| {
            let label = text(&choice["label"]);
            let key = choice["id"]
                .as_str()
                .map_or_else(|| label.clone(), str::to_owned);
            (key, label)
        })
        .collect()
}

/// The first line of a message's body, without its Markdown, under its
/// summary, or nothing when the body's first line is already its title.
fn excerpt(message: &Value) -> String {
    if text(&message["summary"]).is_empty() {
        return String::new();
    }
    let line = markdown::first_line(message["body"].as_str().unwrap_or_default());
    let line = line.trim();
    let mut excerpt: String = line.chars().take(EXCERPT).collect();
    if line.chars().count() > EXCERPT {
        excerpt.push('…');
    }
    excerpt
}

/// A party's name as a row says it: its display name, or its handle.
fn name(exchange: &Value, field: &str) -> String {
    exchange[format!("{field}_name")]
        .as_str()
        .map_or_else(|| text(&exchange[field]), str::to_owned)
}

/// Who an exchange is from and where it stands, in words, as `me` reads
/// it: `Alertmanager`, `Claude · You answered: Restart`, or `To Claude ·
/// Waiting for an answer`. An open exchange says nothing of its state,
/// since its section already does.
pub(crate) fn meta(exchange: &Value, me: &str) -> String {
    let sent = exchange["from"] == me;
    let mut parts = Vec::new();
    if sent {
        parts.push(format!("To {}", name(exchange, "to")));
    } else {
        parts.push(name(exchange, "from"));
        if exchange["to"] != me {
            parts.push(format!("to {}", name(exchange, "to")));
        }
    }
    let question = exchange["question"] == true;
    let state = match exchange["state"].as_str().unwrap_or_default() {
        "open" if sent && question => Some("Waiting for an answer".to_owned()),
        "pending" => Some("Waiting to be accepted".to_owned()),
        "replied" => Some(answered(exchange, me)),
        "expired" if question => Some("Expired unanswered".to_owned()),
        "expired" => Some("Expired".to_owned()),
        "withdrawn" => Some("Withdrawn".to_owned()),
        "declined" => Some("Declined".to_owned()),
        _ => None,
    };
    parts.extend(state);
    parts.join(" · ")
}

/// Who answered and what: `You answered: Restart`, or `Anna answered:
/// Leave it`, by the choice's label, or the text when there is no choice.
fn answered(exchange: &Value, me: &str) -> String {
    let reply = &exchange["reply"];
    let who = if reply["by"] == me {
        "You".to_owned()
    } else {
        reply["by_name"]
            .as_str()
            .map_or_else(|| text(&reply["by"]), str::to_owned)
    };
    let said = reply["choice"].as_str().map_or_else(
        || text(&reply["text"]),
        |key| {
            exchange["message"]["choices"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|choice| {
                    choice["id"] == key || (choice["id"].is_null() && choice["label"] == key)
                })
                .map_or_else(|| key.to_owned(), |choice| text(&choice["label"]))
        },
    );
    format!("{who} answered: {said}")
}

/// How long ago `at` was, as a row says it: `now`, `4m`, `3h`, `Yesterday`,
/// a weekday within the week, then the date, with the year once it is not
/// this one. The days are UTC's here; the page's script says them again
/// in the browser's own time zone.
pub(crate) fn relative(at: Timestamp, now: Timestamp) -> String {
    let seconds = (now - at).get_seconds();
    if seconds < 60 {
        return "now".to_owned();
    }
    if seconds < 60 * 60 {
        return format!("{}m", seconds / 60);
    }
    let then = at.to_zoned(TimeZone::UTC);
    let today = now.to_zoned(TimeZone::UTC).date();
    let days = then.date().until(today).map_or(0, |span| span.get_days());
    if days == 0 {
        format!("{}h", seconds / 3600)
    } else if days == 1 {
        "Yesterday".to_owned()
    } else if days < 7 {
        then.strftime("%a").to_string()
    } else if then.year() == today.year() {
        then.strftime("%b %-d").to_string()
    } else {
        then.strftime("%b %-d, %Y").to_string()
    }
}
