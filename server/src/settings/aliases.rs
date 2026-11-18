//! An issuer's aliases: the subjects that sign in as another subject's
//! individual. Only the settings name them, so no request can attach a
//! token to an individual it was not issued for.

use serde::Deserialize;

/// An issuer's aliases, in the order the settings give them: a list of
/// `pattern=subject`, or one string of them separated by spaces, since an
/// environment variable holds only a string. Neither part holds a space.
#[derive(Clone, Debug, Default)]
pub(crate) struct Aliases(Vec<Alias>);

impl Aliases {
    /// The subject whose individual a token for `subject` signs in as:
    /// the subject of the first alias that matches, or `subject` itself.
    pub(crate) fn subject(&self, subject: String) -> String {
        self.0
            .iter()
            .find(|alias| alias.matches(&subject))
            .map_or(subject, |alias| alias.subject.clone())
    }
}

/// One alias: the subjects that `pattern` matches sign in as `subject`.
#[derive(Clone, Debug)]
struct Alias {
    pattern: String,
    subject: String,
}

impl Alias {
    /// A pattern that ends in `*` matches every subject that starts with
    /// the rest of it. Any other pattern matches the one subject equal to
    /// it.
    fn matches(&self, subject: &str) -> bool {
        match self.pattern.strip_suffix('*') {
            Some(start) => subject.starts_with(start),
            None => subject == self.pattern,
        }
    }
}

impl std::str::FromStr for Alias {
    type Err = String;

    fn from_str(alias: &str) -> Result<Self, Self::Err> {
        match alias.split_once('=') {
            Some((pattern, subject)) if !pattern.is_empty() && !subject.is_empty() => Ok(Self {
                pattern: pattern.to_owned(),
                subject: subject.to_owned(),
            }),
            _ => Err(format!("the alias {alias:?} is not pattern=subject")),
        }
    }
}

impl std::str::FromStr for Aliases {
    type Err = String;

    fn from_str(aliases: &str) -> Result<Self, Self::Err> {
        aliases
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map(Self)
    }
}

impl<'de> Deserialize<'de> for Aliases {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Written {
            List(Vec<String>),
            Spaced(String),
        }
        match Written::deserialize(deserializer)? {
            Written::List(aliases) => aliases.join(" ").parse(),
            Written::Spaced(aliases) => aliases.parse(),
        }
        .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Aliases;

    /// The subject each of `subjects` signs in as, under the aliases that
    /// `written` holds as a settings file or the environment gives them.
    fn aliased(written: serde_json::Value, subjects: &[&str]) -> Option<Vec<String>> {
        let aliases: Aliases = serde_json::from_value(written).ok()?;
        Some(
            subjects
                .iter()
                .map(|subject| aliases.subject((*subject).to_owned()))
                .collect(),
        )
    }

    fn subjects(subjects: &[&str]) -> Vec<String> {
        subjects
            .iter()
            .map(|subject| (*subject).to_owned())
            .collect()
    }

    #[test]
    fn a_subject_is_itself_without_aliases() {
        assert_eq!(
            Aliases::default().subject("agent".to_owned()),
            "agent".to_owned()
        );
    }

    #[test]
    fn a_list_names_one_subject_or_every_subject_with_a_start() {
        assert_eq!(
            aliased(
                json!(["agent:reviewer=agent", "agent--*=agent"]),
                &[
                    "agent:reviewer",
                    "agent--fixer",
                    "agent",
                    "agent-two",
                    "other"
                ]
            ),
            Some(subjects(&["agent", "agent", "agent", "agent-two", "other"]))
        );
    }

    #[test]
    fn one_string_separates_aliases_by_spaces() {
        assert_eq!(
            aliased(json!("a--*=a b--*=b"), &["a--one", "b--one", "c--one"]),
            Some(subjects(&["a", "b", "c--one"]))
        );
    }

    #[test]
    fn the_first_alias_that_matches_decides() {
        assert_eq!(
            aliased(json!("a--x=first a--*=second"), &["a--x", "a--y"]),
            Some(subjects(&["first", "second"]))
        );
    }

    #[test]
    fn rejects_an_alias_that_names_no_subject() {
        assert_eq!(
            [
                aliased(json!("a--*"), &[]),
                aliased(json!("a--*="), &[]),
                aliased(json!("=a"), &[])
            ],
            [None, None, None]
        );
    }
}
