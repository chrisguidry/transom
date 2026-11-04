//! The message: the sender's leg of an exchange, and the rules a message
//! must follow whichever form it arrived in.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// How urgently to deliver a message, with the values of the Web Push
/// protocol's `Urgency` header (RFC 8030).
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Urgency {
    VeryLow,
    Low,
    #[default]
    Normal,
    High,
}

impl Urgency {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "very-low" => Ok(Self::VeryLow),
            "low" => Ok(Self::Low),
            "normal" => Ok(Self::Normal),
            "high" => Ok(Self::High),
            _ => Err(format!(
                "urgency must be very-low, low, normal, or high, not {value:?}"
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct Choice {
    pub(crate) label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<String>,
}

impl Choice {
    /// What a reply names this choice by: its id, or its label without one.
    pub(crate) fn key(&self) -> &str {
        self.id.as_deref().unwrap_or(&self.label)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct Link {
    pub(crate) href: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) rel: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct Message {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) body: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) choices: Vec<Choice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ttl: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) link: Vec<Link>,
    #[serde(default)]
    pub(crate) urgency: Urgency,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) metadata: BTreeMap<String, String>,
}

impl Message {
    /// A message with choices or a prompt asks for a reply.
    pub(crate) fn is_question(&self) -> bool {
        !self.choices.is_empty() || self.prompt.is_some()
    }

    /// Checks the rules every message follows, whatever form it came in.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.summary.as_deref().is_none_or(str::is_empty)
            && self.body.as_deref().is_none_or(str::is_empty)
        {
            return Err("a message needs a summary or a body".to_owned());
        }
        let mut keys = std::collections::HashSet::new();
        for choice in &self.choices {
            if choice.label.is_empty() {
                return Err("every choice needs a label".to_owned());
            }
            if !keys.insert(choice.key()) {
                return Err(format!("two choices are both {:?}", choice.key()));
            }
        }
        if self.ttl == Some(0) {
            return Err("ttl must be at least one second".to_owned());
        }
        for link in &self.link {
            let scheme = url::Url::parse(&link.href)
                .map(|url| url.scheme().to_owned())
                .map_err(|error| format!("{:?} is not a URL: {error}", link.href))?;
            if scheme != "https" && scheme != "http" {
                return Err(format!("{:?} is not an http or https URL", link.href));
            }
        }
        Ok(())
    }

    /// The message without the sender's metadata, as anyone but the sender
    /// sees it.
    pub(crate) fn for_recipient(&self) -> Self {
        Self {
            metadata: BTreeMap::new(),
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Choice, Link, Message, Urgency};

    fn message() -> Message {
        Message {
            summary: Some("Restart postgres?".to_owned()),
            body: None,
            choices: vec![],
            prompt: None,
            ttl: None,
            link: vec![],
            urgency: Urgency::Normal,
            metadata: std::collections::BTreeMap::new(),
        }
    }

    fn choice(label: &str, id: Option<&str>) -> Choice {
        Choice {
            label: label.to_owned(),
            description: None,
            id: id.map(str::to_owned),
        }
    }

    #[test]
    fn a_message_with_choices_or_a_prompt_is_a_question() {
        let notification = message();
        let with_choices = Message {
            choices: vec![choice("Restart", None)],
            ..message()
        };
        let with_prompt = Message {
            prompt: Some(String::new()),
            ..message()
        };
        assert_eq!(
            [&notification, &with_choices, &with_prompt].map(Message::is_question),
            [false, true, true]
        );
    }

    #[test]
    fn accepts_a_valid_message() {
        let valid = Message {
            choices: vec![choice("Restart", Some("restart")), choice("Leave it", None)],
            ttl: Some(60),
            link: vec![Link {
                href: "https://grafana.example.com/d/disk".to_owned(),
                rel: Some("related".to_owned()),
            }],
            ..message()
        };
        assert_eq!(valid.validate(), Ok(()));
    }

    #[test]
    fn rejects_invalid_messages() {
        let link = |href: &str| Link {
            href: href.to_owned(),
            rel: None,
        };
        let invalid = [
            Message {
                summary: None,
                ..message()
            },
            Message {
                summary: Some(String::new()),
                body: Some(String::new()),
                ..message()
            },
            Message {
                choices: vec![choice("", None)],
                ..message()
            },
            Message {
                choices: vec![choice("Restart", Some("go")), choice("Go", Some("go"))],
                ..message()
            },
            Message {
                ttl: Some(0),
                ..message()
            },
            Message {
                link: vec![link("not a url")],
                ..message()
            },
            Message {
                link: vec![link("javascript:alert(1)")],
                ..message()
            },
        ];
        for message in invalid {
            assert!(message.validate().is_err(), "{message:?}");
        }
    }

    #[test]
    fn hides_the_metadata_from_recipients() {
        let mut sent = message();
        sent.metadata.insert("session".to_owned(), "abc".to_owned());
        assert!(sent.for_recipient().metadata.is_empty());
    }

    #[test]
    fn parses_urgencies() {
        assert_eq!(
            ["very-low", "low", "normal", "high"].map(|value| Urgency::parse(value).unwrap()),
            [
                Urgency::VeryLow,
                Urgency::Low,
                Urgency::Normal,
                Urgency::High
            ]
        );
        assert!(Urgency::parse("urgent").is_err());
    }
}
