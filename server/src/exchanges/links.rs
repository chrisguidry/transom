//! The `Link` header of RFC 8288: one or more `<uri>` targets, each with
//! parameters such as `rel="related"`, separated by commas. Only `rel` and
//! `title` are kept; transom has no use for the other parameters.

use super::message::Link;

/// Reads past spaces and tabs.
fn skip_space(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while chars.next_if(|c| *c == ' ' || *c == '\t').is_some() {}
}

/// A parameter value: a quoted string, with backslash escapes, or a token.
fn value(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, String> {
    let mut value = String::new();
    if chars.next_if_eq(&'"').is_some() {
        loop {
            match chars.next() {
                Some('"') => return Ok(value),
                Some('\\') => value.push(chars.next().ok_or("Link ends inside an escape")?),
                Some(c) => value.push(c),
                None => return Err("Link has an unclosed quoted string".to_owned()),
            }
        }
    }
    while let Some(c) = chars.next_if(|c| !matches!(c, ';' | ',' | ' ' | '\t')) {
        value.push(c);
    }
    Ok(value)
}

pub(crate) fn parse(header: &str) -> Result<Vec<Link>, String> {
    let mut links = Vec::new();
    let mut chars = header.chars().peekable();
    loop {
        skip_space(&mut chars);
        if chars.next() != Some('<') {
            return Err("each Link must start with a <uri>".to_owned());
        }
        let href: String = chars.by_ref().take_while(|c| *c != '>').collect();
        let mut rel = None;
        let mut title = None;
        loop {
            skip_space(&mut chars);
            match chars.next() {
                Some(';') => {
                    skip_space(&mut chars);
                    let mut name = String::new();
                    while let Some(c) =
                        chars.next_if(|c| c.is_ascii_alphanumeric() || "-_*".contains(*c))
                    {
                        name.push(c);
                    }
                    skip_space(&mut chars);
                    let parameter = if chars.next_if_eq(&'=').is_some() {
                        skip_space(&mut chars);
                        value(&mut chars)?
                    } else {
                        String::new()
                    };
                    if name.eq_ignore_ascii_case("rel") {
                        rel = Some(parameter);
                    } else if name.eq_ignore_ascii_case("title") {
                        title = Some(parameter);
                    }
                }
                Some(',') => break,
                None => {
                    links.push(Link { href, rel, title });
                    return Ok(links);
                }
                Some(other) => return Err(format!("Link has {other:?} where ; or , belongs")),
            }
        }
        links.push(Link { href, rel, title });
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::exchanges::message::Link;

    fn link(href: &str, rel: Option<&str>, title: Option<&str>) -> Link {
        Link {
            href: href.to_owned(),
            rel: rel.map(str::to_owned),
            title: title.map(str::to_owned),
        }
    }

    #[test]
    fn parses_links_with_their_relations_and_titles() {
        let header = r#"<https://grafana/d/disk>; rel="related"; title="Disk", <https://runbooks/disk>;rel=help , <https://silence>"#;
        assert_eq!(
            parse(header).unwrap(),
            vec![
                link("https://grafana/d/disk", Some("related"), Some("Disk")),
                link("https://runbooks/disk", Some("help"), None),
                link("https://silence", None, None),
            ]
        );
    }

    #[test]
    fn reads_escapes_and_bare_parameters() {
        assert_eq!(
            parse(r#"<https://a>; crossorigin; REL="say \"hi\"""#).unwrap(),
            vec![link("https://a", Some("say \"hi\""), None)]
        );
    }

    #[test]
    fn rejects_malformed_links() {
        for header in [
            "https://no-brackets",
            r#"<https://a>; rel="unclosed"#,
            r#"<https://a>; rel="ends\"#,
            "<https://a> stray",
        ] {
            assert!(parse(header).is_err(), "{header}");
        }
    }
}
