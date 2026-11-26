//! An exchange in words, as the web's inbox and the Android app say it:
//! its title, the line under it, when it arrived, and where it stands.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use super::{Exchange, Reply, State};
use crate::inbox::{Entry, Sending};

const TITLE_LENGTH: usize = 80;
const EXCERPT_LENGTH: usize = 120;

/// The first line of `markdown`'s text, with its marks taken out, as a
/// list shows a message under its title: `**94%** full` reads `94% full`,
/// a list item loses its bullet, and a link keeps its words.
pub fn first_line(markdown: &str) -> String {
    let mut line = String::new();
    for event in Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH) {
        let ends_a_line = matches!(
            event,
            Event::SoftBreak
                | Event::HardBreak
                | Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item)
                | Event::Start(Tag::CodeBlock(_))
        );
        if ends_a_line && !line.trim().is_empty() {
            break;
        }
        match event {
            Event::Text(text) | Event::Code(text) | Event::InlineHtml(text) => {
                line.push_str(&text);
            }
            Event::Html(text) => line.push_str(text.trim()),
            _ => {}
        }
    }
    line.lines().next().unwrap_or("").trim().to_owned()
}

fn cut(line: &str, most: usize) -> String {
    if line.chars().count() <= most {
        return line.to_owned();
    }
    let mut cut: String = line.chars().take(most).collect();
    cut.push('…');
    cut
}

/// What a message is called in a list and over its own page: its summary,
/// or the first line of its body without its Markdown, cut at 80
/// characters.
pub fn title(exchange: &Exchange) -> String {
    match exchange.message.summary.as_deref() {
        Some(summary) if !summary.is_empty() => summary.to_owned(),
        _ => cut(
            &first_line(exchange.message.body.as_deref().unwrap_or("")),
            TITLE_LENGTH,
        ),
    }
}

/// The first line of the body, to show under the summary in a list, or
/// nothing when the body's first line is already the title.
pub fn excerpt(exchange: &Exchange) -> String {
    match exchange.message.summary.as_deref() {
        Some(summary) if !summary.is_empty() => cut(
            &first_line(exchange.message.body.as_deref().unwrap_or("")),
            EXCERPT_LENGTH,
        ),
        _ => String::new(),
    }
}

/// How long ago `then` was at `now`, as a row says it: `now`, `4m`, `3h`,
/// `Yesterday`, a weekday within the week, then the date, with the year
/// once it is not this one, by the days of `zone`.
pub fn ago(then: Timestamp, now: Timestamp, zone: &TimeZone) -> String {
    let seconds = now.as_second() - then.as_second();
    if seconds < 60 {
        return "now".to_owned();
    }
    if seconds < 3600 {
        return format!("{}m", seconds / 60);
    }
    let at = then.to_zoned_datetime(zone.clone());
    let today = now.to_zoned_datetime(zone.clone());
    let days = i64::from((today.date() - at.date()).get_days());
    let pattern = match days {
        0 => return format!("{}h", seconds / 3600),
        1 => return "Yesterday".to_owned(),
        2..7 => "%a",
        _ if at.year() == today.year() => "%b %-d",
        _ => "%b %-d, %Y",
    };
    at.strftime(pattern).to_string()
}

/// `then` in full, as an exchange's page says it: `Thu, Nov 5 at 12:29 PM`.
pub fn in_full(then: Timestamp, zone: &TimeZone) -> String {
    then.to_zoned_datetime(zone.clone())
        .strftime("%a, %b %-d at %-I:%M %p")
        .to_string()
}

/// A party as a page names it: `Alertmanager (@alertmanager)`, or the
/// handle alone when it has no display name.
pub fn party(handle: &str, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("{name} ({handle})"),
        None => handle.to_owned(),
    }
}

/// What a reply said: its choice's label, or its text when there is no
/// choice.
pub fn said(exchange: &Exchange, choice: Option<&str>, text: Option<&str>) -> String {
    choice
        .map(|key| {
            exchange
                .message
                .choices
                .iter()
                .find(|choice| choice.key() == key)
                .map_or(key, |choice| choice.label.as_str())
        })
        .or(text)
        .unwrap_or("")
        .to_owned()
}

fn answered(exchange: &Exchange, reply: &Reply, me: &str) -> String {
    let who = if reply.by == me {
        "You"
    } else {
        reply.by_name.as_deref().unwrap_or(&reply.by)
    };
    let said = said(exchange, reply.choice.as_deref(), reply.text.as_deref());
    format!("{who} answered: {said}")
}

/// A reply this device has queued, in words: `Your answer, No, is not
/// sent yet`.
pub fn unsent(exchange: &Exchange, sending: &Sending) -> String {
    let said = said(exchange, sending.choice.as_deref(), sending.text.as_deref());
    format!("Your answer, {said}, is not sent yet")
}

/// A reply that came after someone else's, in words: `Your answer, No,
/// was not used: Sam answered first`.
pub fn lost(exchange: &Exchange, mine: &Sending, winner: &Reply) -> String {
    let said = said(exchange, mine.choice.as_deref(), mine.text.as_deref());
    let who = winner.by_name.as_deref().unwrap_or(&winner.by);
    format!("Your answer, {said}, was not used: {who} answered first")
}

/// Where `entry` stands, in words, as `me` (a handle with its `@`) reads
/// it. An open exchange that waits on `me` says nothing, since its
/// section already does.
pub fn standing(entry: &Entry, me: &str) -> Option<String> {
    let exchange = &entry.exchange;
    if let Some(sending) = &entry.sending {
        return Some(unsent(exchange, sending));
    }
    if let (Some(lost_reply), Some(winner)) = (&entry.lost, &exchange.reply) {
        return Some(lost(exchange, lost_reply, winner));
    }
    match exchange.state {
        State::Open => {
            (exchange.from == me && exchange.question).then(|| "Waiting for an answer".to_owned())
        }
        State::Pending => Some("Waiting to be accepted".to_owned()),
        State::Replied => Some(exchange.reply.as_ref().map_or_else(
            || "Answered".to_owned(),
            |reply| answered(exchange, reply, me),
        )),
        State::Expired => Some(
            if exchange.question {
                "Expired unanswered"
            } else {
                "Expired"
            }
            .to_owned(),
        ),
        State::Withdrawn => Some("Withdrawn".to_owned()),
        State::Declined => Some("Declined".to_owned()),
        State::Replaced => Some("Replaced by a newer message".to_owned()),
    }
}

/// Who `entry` is from and where it stands, as a row says it:
/// `Alertmanager`, `Claude · You answered: Restart`, or `To Claude ·
/// Waiting for an answer`.
pub fn meta(entry: &Entry, me: &str) -> String {
    let exchange = &entry.exchange;
    let mut parts = Vec::new();
    if exchange.from == me {
        parts.push(format!(
            "To {}",
            exchange.to_name.as_deref().unwrap_or(&exchange.to)
        ));
    } else {
        parts.push(exchange.sender().to_owned());
        if exchange.to != me {
            parts.push(format!(
                "to {}",
                exchange.to_name.as_deref().unwrap_or(&exchange.to)
            ));
        }
    }
    parts.extend(standing(entry, me));
    parts.join(" · ")
}

/// Where an exchange stands on its own page, which shows the reply
/// itself, so a reply is only `Answered` there.
pub fn standing_on_page(entry: &Entry) -> Option<String> {
    if entry.exchange.state == State::Replied {
        return Some("Answered".to_owned());
    }
    let settled = Entry {
        sending: None,
        lost: None,
        ..entry.clone()
    };
    standing(&settled, "")
}

/// The line over an exchange's page: `Alertmanager (@alertmanager) to
/// Chris (@chris) · Thu, Nov 5 at 12:29 PM`, and where it stands once it
/// is no longer open.
pub fn page_meta(entry: &Entry, zone: &TimeZone) -> String {
    let exchange = &entry.exchange;
    let mut line = format!(
        "{} to {} · {}",
        party(&exchange.from, exchange.from_name.as_deref()),
        party(&exchange.to, exchange.to_name.as_deref()),
        in_full(exchange.created, zone),
    );
    if let Some(standing) = standing_on_page(entry) {
        line.push_str(" · ");
        line.push_str(&standing);
    }
    line
}

#[cfg(test)]
mod tests;
