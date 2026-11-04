//! The fields of a message as they arrive from one source: a structured
//! body, a form, the query string, or the headers. Each source fills in
//! what it has, and merging the sources refuses a field given two ways.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::links;
use super::message::{Choice, Link, Message, Urgency};

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Fields {
    pub(crate) summary: Option<String>,
    pub(crate) body: Option<String>,
    pub(crate) choices: Option<Vec<Choice>>,
    pub(crate) prompt: Option<String>,
    pub(crate) ttl: Option<u64>,
    pub(crate) link: Option<Vec<Link>>,
    pub(crate) urgency: Option<Urgency>,
    pub(crate) metadata: Option<BTreeMap<String, String>>,
}

/// Whether `name` is a field of a message in a form or a query string.
/// `wait` belongs to the request rather than the message, but may share
/// a query string with them.
pub(crate) fn is_field(name: &str) -> bool {
    matches!(
        name,
        "summary" | "body" | "choices" | "prompt" | "ttl" | "link" | "urgency" | "wait"
    ) || name.starts_with("metadata.")
}

/// Takes `ours` or `theirs`, whichever is given, and refuses both given
/// with different values.
fn either<T: PartialEq>(
    name: &str,
    ours: Option<T>,
    theirs: Option<T>,
) -> Result<Option<T>, String> {
    match (ours, theirs) {
        (Some(ours), Some(theirs)) if ours != theirs => {
            Err(format!("{name} is given twice with different values"))
        }
        (Some(value), _) | (None, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

impl Fields {
    pub(crate) fn merge(self, other: Self) -> Result<Self, String> {
        Ok(Self {
            summary: either("summary", self.summary, other.summary)?,
            body: either("body", self.body, other.body)?,
            choices: either("choices", self.choices, other.choices)?,
            prompt: either("prompt", self.prompt, other.prompt)?,
            ttl: either("ttl", self.ttl, other.ttl)?,
            link: either("link", self.link, other.link)?,
            urgency: either("urgency", self.urgency, other.urgency)?,
            metadata: either("metadata", self.metadata, other.metadata)?,
        })
    }

    pub(crate) fn into_message(self) -> Result<Message, String> {
        let message = Message {
            summary: self.summary,
            body: self.body,
            choices: self.choices.unwrap_or_default(),
            prompt: self.prompt,
            ttl: self.ttl,
            link: self.link.unwrap_or_default(),
            urgency: self.urgency.unwrap_or_default(),
            metadata: self.metadata.unwrap_or_default(),
        };
        message.validate()?;
        Ok(message)
    }

    /// Fields from name and value pairs, as a form or a query string
    /// carries them. A repeated `choices` or `link` adds one more, and
    /// `metadata.{key}` sets one entry of the metadata.
    pub(crate) fn from_pairs(pairs: &[(String, String)]) -> Result<Self, String> {
        let mut fields = Self::default();
        for (name, value) in pairs {
            let value = value.as_str();
            match name.as_str() {
                "summary" => fields.summary = Some(value.to_owned()),
                "body" => fields.body = Some(value.to_owned()),
                "prompt" => fields.prompt = Some(value.to_owned()),
                "ttl" => fields.ttl = Some(ttl(value)?),
                "urgency" => fields.urgency = Some(Urgency::parse(value)?),
                "choices" => fields.choices.get_or_insert_default().push(Choice {
                    label: value.to_owned(),
                    description: None,
                    id: None,
                }),
                "link" => fields.link.get_or_insert_default().push(Link {
                    href: value.to_owned(),
                    rel: None,
                }),
                "wait" => {}
                name => match name.strip_prefix("metadata.") {
                    Some(key) => {
                        fields
                            .metadata
                            .get_or_insert_default()
                            .insert(key.to_owned(), value.to_owned());
                    }
                    None => return Err(format!("{name} is not a field of a message")),
                },
            }
        }
        Ok(fields)
    }

    /// Fields from the headers of the same names. `Choices` is a
    /// Structured Fields list, `Metadata` a Structured Fields dictionary,
    /// and `Link` the header of RFC 8288.
    pub(crate) fn from_headers(headers: &axum::http::HeaderMap) -> Result<Self, String> {
        // A header that is not valid UTF-8 keeps what can be read of it,
        // rather than failing the whole send over one character.
        let text = |name: &str| -> Result<Option<String>, String> {
            Ok(headers
                .get(name)
                .map(|value| String::from_utf8_lossy(value.as_bytes()).trim().to_owned()))
        };
        Ok(Self {
            summary: text("summary")?,
            body: None,
            choices: text("choices")?.map(|value| choices(&value)).transpose()?,
            prompt: text("prompt")?,
            ttl: text("ttl")?.map(|value| ttl(&value)).transpose()?,
            link: text("link")?
                .map(|value| links::parse(&value))
                .transpose()?,
            urgency: text("urgency")?
                .map(|value| Urgency::parse(&value))
                .transpose()?,
            metadata: text("metadata")?
                .map(|value| metadata(&value))
                .transpose()?,
        })
    }
}

/// A time to live: whole seconds, as RFC 8030 writes it, or a duration
/// such as `1h` or `90m`.
pub(crate) fn ttl(value: &str) -> Result<u64, String> {
    if let Ok(seconds) = value.parse::<u64>() {
        return Ok(seconds);
    }
    let span: jiff::Span = value
        .parse()
        .map_err(|_| format!("ttl must be seconds or a duration such as 1h, not {value:?}"))?;
    let seconds = span
        .total(jiff::Unit::Second)
        .map_err(|_| format!("ttl {value:?} is not a fixed length of time"))?;
    // A duration's seconds are whole numbers, well within u64.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(seconds.max(0.0) as u64)
}

/// A Structured Fields list of labels, strings or tokens.
fn choices(value: &str) -> Result<Vec<Choice>, String> {
    let list: sfv::List = sfv::Parser::new(value)
        .parse()
        .map_err(|error| format!("Choices is not a Structured Fields list: {error}"))?;
    list.into_iter()
        .map(|entry| match entry {
            sfv::ListEntry::Item(item) => text_of(&item.bare_item).ok_or(()),
            sfv::ListEntry::InnerList(_) => Err(()),
        })
        .map(|label| {
            label
                .map(|label| Choice {
                    label,
                    description: None,
                    id: None,
                })
                .map_err(|()| "each choice in Choices must be a string or a token".to_owned())
        })
        .collect()
}

/// A Structured Fields dictionary of strings, tokens, or integers.
fn metadata(value: &str) -> Result<BTreeMap<String, String>, String> {
    let dictionary: sfv::Dictionary = sfv::Parser::new(value)
        .parse()
        .map_err(|error| format!("Metadata is not a Structured Fields dictionary: {error}"))?;
    dictionary
        .into_iter()
        .map(|(key, entry)| {
            let value = match entry {
                sfv::ListEntry::Item(item) => text_of(&item.bare_item),
                sfv::ListEntry::InnerList(_) => None,
            };
            value
                .map(|value| (key.as_str().to_owned(), value))
                .ok_or_else(|| {
                    format!(
                        "Metadata {} must be a string, token, or integer",
                        key.as_str()
                    )
                })
        })
        .collect()
}

fn text_of(item: &sfv::BareItem) -> Option<String> {
    match item {
        sfv::BareItem::String(value) => Some(value.as_str().to_owned()),
        sfv::BareItem::Token(value) => Some(value.as_str().to_owned()),
        sfv::BareItem::Integer(value) => Some(i64::from(*value).to_string()),
        sfv::BareItem::DisplayString(value) => Some(value.clone()),
        _ => None,
    }
}

/// A message as a JSON or YAML body carries it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Document {
    summary: Option<String>,
    body: Option<String>,
    choices: Option<Vec<ChoiceDocument>>,
    prompt: Option<String>,
    ttl: Option<TtlDocument>,
    link: Option<Vec<LinkDocument>>,
    urgency: Option<Urgency>,
    metadata: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ChoiceDocument {
    Label(String),
    Choice(Choice),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TtlDocument {
    Seconds(u64),
    Duration(String),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LinkDocument {
    Href(String),
    Link(Link),
}

impl TryFrom<Document> for Fields {
    type Error = String;

    fn try_from(document: Document) -> Result<Self, String> {
        Ok(Self {
            summary: document.summary,
            body: document.body,
            choices: document.choices.map(|choices| {
                choices
                    .into_iter()
                    .map(|choice| match choice {
                        ChoiceDocument::Label(label) => Choice {
                            label,
                            description: None,
                            id: None,
                        },
                        ChoiceDocument::Choice(choice) => choice,
                    })
                    .collect()
            }),
            prompt: document.prompt,
            ttl: document
                .ttl
                .map(|ttl_document| match ttl_document {
                    TtlDocument::Seconds(seconds) => Ok(seconds),
                    TtlDocument::Duration(duration) => ttl(&duration),
                })
                .transpose()?,
            link: document.link.map(|links| {
                links
                    .into_iter()
                    .map(|link| match link {
                        LinkDocument::Href(href) => Link { href, rel: None },
                        LinkDocument::Link(link) => link,
                    })
                    .collect()
            }),
            urgency: document.urgency,
            metadata: document.metadata,
        })
    }
}
