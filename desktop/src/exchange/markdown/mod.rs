//! A message's Markdown, by the web UI's rules: raw HTML shown as text,
//! and links only to `http`, `https`, and `mailto`. `widgets` draws a
//! message's body, and `inline` writes a choice's description on one
//! line.

pub mod widgets;

use std::fmt::Write as _;

use gtk::glib;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Whether a link may be followed: the schemes the web's pages allow.
fn allowed(address: &str) -> bool {
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| address.to_ascii_lowercase().starts_with(scheme))
}

fn escape(text: &str) -> String {
    glib::markup_escape_text(text).to_string()
}

/// `markdown` on one line of Pango markup, for a choice's description
/// beside its label.
pub fn inline(markdown: &str) -> String {
    let mut markup = String::new();
    for event in Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH) {
        match event {
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                markup.push_str(&escape(&text));
            }
            Event::Code(text) => {
                let _ = write!(markup, "<tt>{}</tt>", escape(&text));
            }
            Event::Start(Tag::Emphasis) => markup.push_str("<i>"),
            Event::End(TagEnd::Emphasis) => markup.push_str("</i>"),
            Event::Start(Tag::Strong) => markup.push_str("<b>"),
            Event::End(TagEnd::Strong) => markup.push_str("</b>"),
            Event::Start(Tag::Strikethrough) => markup.push_str("<s>"),
            Event::End(TagEnd::Strikethrough) => markup.push_str("</s>"),
            Event::SoftBreak | Event::HardBreak | Event::End(TagEnd::Paragraph) => markup.push(' '),
            _ => {}
        }
    }
    markup.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_and_mail_links_may_be_followed() {
        for address in [
            "https://example.com",
            "HTTP://example.com",
            "mailto:a@example.com",
        ] {
            assert!(allowed(address), "{address}");
        }
        for address in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "sh.transom:/callback",
        ] {
            assert!(!allowed(address), "{address}");
        }
    }

    #[test]
    fn emphasis_and_strikethrough_are_markup_too() {
        assert_eq!(inline("*soon*, not ~~now~~"), "<i>soon</i>, not <s>now</s>");
    }

    #[test]
    fn a_description_is_one_line_of_markup() {
        assert_eq!(
            inline("Restarts **now**,\nwith `kill -9` & <b>no</b> warning."),
            "Restarts <b>now</b>, with <tt>kill -9</tt> &amp; &lt;b&gt;no&lt;/b&gt; warning."
        );
    }
}
