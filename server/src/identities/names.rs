//! The rules for names. A name follows the rules for a DNS hostname: one or
//! more labels of lowercase letters, digits, and hyphens, joined by dots.
//! That keeps every name safe in a URL without escaping, and makes an
//! atproto handle such as `guid.foo` a valid name.

use std::fmt;

const MAX_NAME: usize = 253;
const MAX_LABEL: usize = 63;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Name(String);

impl Name {
    pub(crate) fn parse(name: &str) -> Result<Self, String> {
        let valid = !name.is_empty()
            && name.len() <= MAX_NAME
            && name.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= MAX_LABEL
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
            });
        if valid {
            Ok(Self(name.to_owned()))
        } else {
            Err(format!(
                "{name:?} is not a valid name: use lowercase letters, digits, and hyphens, \
                 in labels joined by dots"
            ))
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::Name;

    #[test]
    fn accepts_hostname_like_names() {
        for name in [
            "chris",
            "review-bot",
            "guid.foo",
            "a1",
            "x",
            &"a".repeat(63),
        ] {
            assert_eq!(Name::parse(name).unwrap().as_str(), name);
        }
    }

    #[test]
    fn rejects_everything_else() {
        let too_long = vec!["a".repeat(63); 4].join(".");
        for name in [
            "",
            "Chris",
            "review_bot",
            "-chris",
            "chris-",
            "guid..foo",
            ".foo",
            "chris@home",
            &"a".repeat(64),
            &too_long,
        ] {
            assert!(Name::parse(name).is_err(), "{name:?}");
        }
    }

    #[test]
    fn displays_as_itself() {
        assert_eq!(Name::parse("guid.foo").unwrap().to_string(), "guid.foo");
    }
}
