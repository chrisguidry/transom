//! An exchange as the server describes it: the sender's message and, once
//! there is one, the reply. `GET /exchanges/{id}`, the list at
//! `/@{name}/exchanges`, and its live feed all give this shape.

pub mod describe;
pub mod markdown;
pub mod screen;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// transom's own identity, which sends consent requests and invitations.
pub const TRANSOM: &str = "@transom";

/// One exchange. `from` and `to` are names with their `@`. `url` is
/// absolute only when the server knows its public URL. `read` is this
/// person's private read state, which the list gives only to the
/// exchange's recipients.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Exchange {
    pub id: String,
    pub url: String,
    pub state: State,
    pub question: bool,
    pub from: String,
    pub to: String,
    pub created: Timestamp,
    pub message: Message,
    #[serde(default)]
    pub reply: Option<Reply>,
    #[serde(default)]
    pub expires: Option<Timestamp>,
    #[serde(default)]
    pub read: Option<bool>,
    #[serde(default)]
    pub from_name: Option<String>,
    #[serde(default)]
    pub from_picture: Option<String>,
    #[serde(default)]
    pub to_name: Option<String>,
    #[serde(default)]
    pub to_picture: Option<String>,
    /// The exchange with the same key that this one ended as `replaced`.
    #[serde(default)]
    pub replaces: Option<String>,
    /// How many times the server has shown an urgent exchange again
    /// while it went unread.
    #[serde(default)]
    pub repeats: u32,
}

impl Exchange {
    /// The disposition the exchange's badge shows.
    pub fn disposition(&self) -> Disposition {
        self.message.disposition.unwrap_or(if self.question {
            Disposition::Question
        } else {
            Disposition::Informational
        })
    }

    /// The sender as clients show it: its display name, or its handle.
    pub fn sender(&self) -> &str {
        self.from_name.as_deref().unwrap_or(&self.from)
    }
}

/// Where an exchange stands: `pending` → `open` → one of the five final
/// states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Held while the recipient decides whether to accept its sender.
    Pending,
    Open,
    Replied,
    Expired,
    Withdrawn,
    Declined,
    /// Ended by a later message from the same sender, to the same
    /// recipient, with the same key.
    Replaced,
}

/// The recipient's leg: a choice, text, or both.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Reply {
    #[serde(default)]
    pub choice: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    pub by: String,
    pub at: Timestamp,
    #[serde(default)]
    pub by_name: Option<String>,
    #[serde(default)]
    pub by_picture: Option<String>,
}

/// The sender's leg, which never changes once sent. A message with
/// choices or a prompt is a question; an empty prompt still makes one,
/// with a text box and no label.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct Message {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default, rename = "link", deserialize_with = "nullable")]
    pub links: Vec<Link>,
    #[serde(default, deserialize_with = "nullable")]
    pub urgency: Urgency,
    #[serde(default)]
    pub disposition: Option<Disposition>,
    #[serde(default, deserialize_with = "nullable")]
    pub attachments: Vec<Attachment>,
    /// The sender's name for what the message is about. A later message
    /// from the same sender to the same recipient with the same key
    /// replaces this one while it is open.
    #[serde(default)]
    pub key: Option<String>,
}

/// Reads `null` as the field's default: the server writes
/// `"attachments": null` for a message without any.
fn nullable<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// One of a question's choices, with an optional Markdown description.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Choice {
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

impl Choice {
    /// What a reply names this choice by: its id, or its label when it
    /// has none.
    pub fn key(&self) -> &str {
        self.id.as_deref().unwrap_or(&self.label)
    }
}

/// A link that goes with the message, and the title that labels it.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Link {
    pub href: String,
    #[serde(default)]
    pub rel: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

impl Link {
    /// What a button or a line calls the link: its title, or its address.
    pub fn label(&self) -> &str {
        self.title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(&self.href)
    }
}

/// An image sent with the message, which the server serves behind the
/// same sign-in as the message.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Attachment {
    pub name: String,
    #[serde(rename = "type")]
    pub content_type: String,
    pub size: u64,
    pub url: String,
}

/// What kind of news a message is, which its sender's picture carries as
/// a badge. It is separate from the urgency, which decides how the
/// message is delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    Informational,
    Problem,
    Resolved,
    Question,
}

/// How urgently to deliver a message, from least to most urgent. A level
/// a later server adds reads as `normal`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Urgency {
    VeryLow,
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

impl<'de> Deserialize<'de> for Urgency {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "very-low" => Self::VeryLow,
            "low" => Self::Low,
            "high" => Self::High,
            "urgent" => Self::Urgent,
            _ => Self::Normal,
        })
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    /// An open notification from `@alice` to `@bob`, for tests to change.
    pub(crate) fn exchange(id: &str) -> Exchange {
        Exchange {
            id: id.to_owned(),
            url: format!("/exchanges/{id}"),
            state: State::Open,
            question: false,
            from: "@alice".to_owned(),
            to: "@bob".to_owned(),
            created: "2026-11-19T12:00:00Z".parse().unwrap(),
            message: Message {
                summary: Some(format!("Summary of {id}")),
                ..Message::default()
            },
            reply: None,
            expires: None,
            read: Some(false),
            from_name: None,
            from_picture: None,
            to_name: None,
            to_picture: None,
            replaces: None,
            repeats: 0,
        }
    }

    /// `exchange` as a question with `labels` as its choices.
    pub(crate) fn question(id: &str, labels: &[&str]) -> Exchange {
        let mut exchange = exchange(id);
        exchange.question = true;
        exchange.message.choices = labels
            .iter()
            .map(|label| Choice {
                label: (*label).to_owned(),
                description: None,
                id: None,
            })
            .collect();
        exchange
    }

    pub(crate) fn reply(by: &str, choice: Option<&str>, text: Option<&str>) -> Reply {
        Reply {
            choice: choice.map(str::to_owned),
            text: text.map(str::to_owned),
            by: by.to_owned(),
            at: "2026-11-19T12:05:00Z".parse().unwrap(),
            by_name: None,
            by_picture: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exchange_reads_what_the_server_writes() {
        let exchange: Exchange = serde_json::from_str(
            r#"{"id": "01JB", "url": "/exchanges/01JB", "state": "open", "question": true,
                "from": "@alice", "to": "@bob", "created": "2026-11-04T09:30:00.123Z",
                "message": {"summary": "Restart?", "body": null, "attachments": null,
                            "choices": [{"label": "Restart", "id": "restart"}, {"label": "Leave it"}],
                            "urgency": "louder-than-anything", "link": null, "new_field": 1},
                "reply": null, "read": false}"#,
        )
        .unwrap();
        assert_eq!(exchange.state, State::Open);
        assert_eq!(exchange.message.urgency, Urgency::Normal);
        assert_eq!(exchange.message.choices[0].key(), "restart");
        assert_eq!(exchange.message.choices[1].key(), "Leave it");
        assert_eq!(exchange.disposition(), Disposition::Question);
        assert_eq!(exchange.message.attachments, Vec::<Attachment>::new());
    }

    #[test]
    fn a_message_that_names_no_disposition_is_a_question_or_informational() {
        let mut told = testing::exchange("a");
        assert_eq!(told.disposition(), Disposition::Informational);
        assert_eq!(
            testing::question("b", &["Yes"]).disposition(),
            Disposition::Question
        );
        told.message.disposition = Some(Disposition::Problem);
        assert_eq!(told.disposition(), Disposition::Problem);
    }

    #[test]
    fn a_title_falls_back_to_the_body() {
        let mut exchange = testing::exchange("a");
        exchange.message.summary = None;
        exchange.message.body = Some("# Backup finished\n\nIn 4 minutes.".to_owned());
        assert_eq!(describe::title(&exchange), "Backup finished");
    }
}
