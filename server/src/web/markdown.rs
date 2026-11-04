//! Markdown from senders, rendered for the browser without letting a
//! message run script or call out of the page.

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html};

/// Whether a link may point at `url`: the web, mail, or a page of
/// transom's own.
fn allowed(url: &str) -> bool {
    url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("mailto:")
        || (url.starts_with('/') && !url.starts_with("//"))
        || url.starts_with('#')
}

/// Renders `markdown` to HTML. Raw HTML in it is shown as text. A link to
/// anything but the web, mail, or transom itself is shown as its text. An
/// image is shown as a link to it, rather than loaded, so a message cannot
/// learn when it is read by loading one.
pub(crate) fn render(markdown: &str) -> String {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut dropped_links = 0_usize;
    let events = Parser::new_ext(markdown, options).filter_map(|event| match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Some(Event::Text(raw)),
        Event::Start(Tag::Link { dest_url, .. }) if !allowed(&dest_url) => {
            dropped_links += 1;
            None
        }
        Event::End(TagEnd::Link) if dropped_links > 0 => {
            dropped_links -= 1;
            None
        }
        Event::Start(Tag::Image {
            dest_url,
            title,
            id,
            link_type,
        }) => Some(Event::Start(Tag::Link {
            link_type,
            dest_url: if allowed(&dest_url) {
                dest_url
            } else {
                CowStr::from("#")
            },
            title,
            id,
        })),
        Event::End(TagEnd::Image) => Some(Event::End(TagEnd::Link)),
        other => Some(other),
    });
    let mut rendered = String::new();
    html::push_html(&mut rendered, events);
    rendered
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn renders_markdown() {
        assert_eq!(
            render("The disk is at **94%**."),
            "<p>The disk is at <strong>94%</strong>.</p>\n"
        );
    }

    #[test]
    fn shows_raw_html_as_text() {
        let rendered = render("<script>alert(1)</script> and <b>bold</b>");
        assert!(!rendered.contains("<script>"), "{rendered}");
        assert!(rendered.contains("&lt;script&gt;"), "{rendered}");
    }

    #[test]
    fn keeps_only_safe_links() {
        let rendered = render(
            "[web](https://grafana/d) [page](/exchanges/1) [mail](mailto:a@b) [bad](javascript:alert(1)) [far](//evil)",
        );
        assert!(
            rendered.contains(r#"<a href="https://grafana/d">web</a>"#),
            "{rendered}"
        );
        assert!(
            rendered.contains(r#"<a href="/exchanges/1">page</a>"#),
            "{rendered}"
        );
        assert!(
            rendered.contains(r#"<a href="mailto:a@b">mail</a>"#),
            "{rendered}"
        );
        assert!(!rendered.contains("javascript"), "{rendered}");
        assert!(!rendered.contains("//evil"), "{rendered}");
        assert!(
            rendered.contains("bad") && rendered.contains("far"),
            "{rendered}"
        );
    }

    #[test]
    fn shows_images_as_links_instead_of_loading_them() {
        let rendered =
            render("![graph](https://tracker.example.com/pixel.png) ![bad](javascript:x)");
        assert!(!rendered.contains("<img"), "{rendered}");
        assert!(
            rendered.contains(r#"<a href="https://tracker.example.com/pixel.png">graph</a>"#),
            "{rendered}"
        );
        assert!(rendered.contains(r##"<a href="#">bad</a>"##), "{rendered}");
    }
}
