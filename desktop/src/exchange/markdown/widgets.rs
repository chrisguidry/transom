//! A message's Markdown as widgets, by the web UI's rules: tables,
//! strikethrough, and task lists; raw HTML shown as text; links only to
//! `http`, `https`, and `mailto`; and images shown as links, except the
//! message's own attachments.

use std::fmt::Write as _;
use std::rc::Rc;

use adw::prelude::*;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use super::{allowed, escape};
use crate::window::Shell;

/// One level of list being drawn: its next number, when it is numbered.
struct List {
    number: Option<u64>,
}

/// The widgets under construction: the boxes that hold blocks, the
/// markup of the line being read, and the table being filled.
struct Builder<'a> {
    shell: &'a Rc<Shell>,
    exchange: &'a str,
    attachments: &'a [String],
    placed: Vec<String>,
    boxes: Vec<gtk::Box>,
    markup: String,
    lists: Vec<List>,
    code: Option<String>,
    table: Option<(gtk::Grid, i32, i32, bool)>,
    /// The classes the next label takes.
    classes: Vec<&'static str>,
    /// Whether a link that may not be followed is open, so its closing
    /// tag is left out too.
    dead_link: bool,
}

impl Builder<'_> {
    fn container(&self) -> &gtk::Box {
        self.boxes.last().expect("the root box is never popped")
    }

    /// Ends the line being read, as a label in the block being built.
    fn flush(&mut self) {
        let markup = std::mem::take(&mut self.markup);
        let classes = std::mem::take(&mut self.classes);
        if markup.trim().is_empty() {
            return;
        }
        let label = gtk::Label::builder()
            .use_markup(true)
            .label(markup.trim_end())
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .selectable(true)
            .build();
        for class in classes {
            label.add_css_class(class);
        }
        if let Some((grid, column, row, head)) = &self.table {
            label.add_css_class("cell");
            if *head {
                label.add_css_class("head");
            }
            grid.attach(&label, *column, *row, 1, 1);
        } else {
            self.container().append(&label);
        }
    }

    fn push(&mut self, container: gtk::Box) {
        self.container().append(&container);
        self.boxes.push(container);
    }

    /// Puts the attachment `name` in the message as a picture, which
    /// fills in once it has been fetched.
    fn picture(&mut self, name: &str, alt: &str) {
        self.flush();
        self.placed.push(name.to_owned());
        self.container()
            .append(&attachment(self.shell, self.exchange, name, alt));
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { .. } => self.classes.push("heading"),
            Tag::BlockQuote(_) => {
                self.flush();
                let quote = block();
                quote.add_css_class("quote");
                self.push(quote);
            }
            Tag::CodeBlock(_) => {
                self.flush();
                self.code = Some(String::new());
            }
            Tag::List(number) => {
                self.flush();
                self.lists.push(List { number });
            }
            Tag::Item => {
                self.flush();
                let mark = match self.lists.last_mut().and_then(|list| list.number.as_mut()) {
                    Some(number) => {
                        *number += 1;
                        format!("{}.", *number - 1)
                    }
                    None => "•".to_owned(),
                };
                let line = gtk::Box::builder().spacing(8).build();
                let bullet = gtk::Label::builder()
                    .label(mark)
                    .valign(gtk::Align::Start)
                    .build();
                bullet.add_css_class("muted");
                line.append(&bullet);
                let item = block();
                item.set_hexpand(true);
                line.append(&item);
                self.container().append(&line);
                self.boxes.push(item);
            }
            Tag::Table(_) => {
                self.flush();
                let grid = gtk::Grid::builder().halign(gtk::Align::Start).build();
                self.container().append(&grid);
                self.table = Some((grid, 0, -1, false));
            }
            Tag::TableHead => {
                if let Some((_, column, row, head)) = &mut self.table {
                    (*column, *row, *head) = (0, *row + 1, true);
                }
            }
            Tag::TableRow => {
                if let Some((_, column, row, head)) = &mut self.table {
                    (*column, *row, *head) = (0, *row + 1, false);
                }
            }
            Tag::Emphasis => self.markup.push_str("<i>"),
            Tag::Strong => self.markup.push_str("<b>"),
            Tag::Strikethrough => self.markup.push_str("<s>"),
            Tag::Link { dest_url, .. } => {
                if allowed(&dest_url) {
                    self.open_link(&dest_url);
                } else {
                    self.dead_link = true;
                }
            }
            Tag::Image { dest_url, .. } => {
                if self.attachments.iter().any(|name| **name == *dest_url) {
                    let name = dest_url.to_string();
                    self.picture(&name, "");
                    self.dead_link = true;
                } else if allowed(&dest_url) {
                    self.open_link(&dest_url);
                } else {
                    self.dead_link = true;
                }
            }
            _ => {}
        }
    }

    /// Opens a link to `address` in the line being read.
    fn open_link(&mut self, address: &str) {
        let _ = write!(self.markup, "<a href=\"{}\">", escape(address));
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) => self.flush(),
            TagEnd::BlockQuote(_) | TagEnd::Item => {
                self.flush();
                self.boxes.pop();
            }
            TagEnd::CodeBlock => {
                let code = self.code.take().unwrap_or_default();
                let label = gtk::Label::builder()
                    .label(code.trim_end())
                    .xalign(0.0)
                    .wrap(true)
                    .wrap_mode(gtk::pango::WrapMode::Char)
                    .selectable(true)
                    .build();
                label.add_css_class("pre");
                self.container().append(&label);
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::TableCell => {
                self.flush();
                if let Some((_, column, _, _)) = &mut self.table {
                    *column += 1;
                }
            }
            TagEnd::Table => self.table = None,
            TagEnd::Emphasis => self.markup.push_str("</i>"),
            TagEnd::Strong => self.markup.push_str("</b>"),
            TagEnd::Strikethrough => self.markup.push_str("</s>"),
            TagEnd::Link | TagEnd::Image if !std::mem::take(&mut self.dead_link) => {
                self.markup.push_str("</a>");
            }
            _ => {}
        }
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => match &mut self.code {
                Some(code) => code.push_str(&text),
                None => self.markup.push_str(&escape(&text)),
            },
            Event::Code(text) => {
                let _ = write!(self.markup, "<tt>{}</tt>", escape(&text));
            }
            Event::Html(text) | Event::InlineHtml(text) => self.markup.push_str(&escape(&text)),
            Event::SoftBreak => self.markup.push(' '),
            Event::HardBreak => self.markup.push('\n'),
            Event::TaskListMarker(done) => self.markup.push_str(if done { "☑ " } else { "☐ " }),
            Event::Rule => {
                self.flush();
                self.container()
                    .append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            }
            _ => {}
        }
    }
}

fn block() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build()
}

/// The picture of `name`, an image attached to `exchange`. `alt` shows
/// until the image has been fetched, and stays if it cannot be.
pub fn attachment(shell: &Rc<Shell>, exchange: &str, name: &str, alt: &str) -> gtk::Box {
    let frame = gtk::Box::builder().halign(gtk::Align::Start).build();
    frame.add_css_class("images");
    let waiting = gtk::Label::new(Some(if alt.is_empty() { name } else { alt }));
    waiting.add_css_class("muted");
    frame.append(&waiting);
    if let Some(pictures) = shell.snapshot().pictures.clone() {
        let (exchange, name, shown) = (exchange.to_owned(), name.to_owned(), frame.clone());
        shell.run(
            async move { pictures.attachment(&exchange, &name).await },
            move |file| {
                let Some(file) = file else { return };
                let picture = gtk::Picture::for_filename(file);
                picture.set_can_shrink(true);
                picture.set_content_fit(gtk::ContentFit::ScaleDown);
                picture.set_halign(gtk::Align::Start);
                shown.remove(&waiting);
                shown.append(&picture);
            },
        );
    }
    frame
}

/// `markdown` as a column of widgets, and the names of the attachments
/// it placed. `attachments` are the names of the message's images, each
/// of which the body may place by naming it, as `![the graph](graph.png)`.
pub fn render(
    shell: &Rc<Shell>,
    markdown: &str,
    exchange: &str,
    attachments: &[String],
) -> (gtk::Box, Vec<String>) {
    let root = block();
    root.add_css_class("body");
    let mut builder = Builder {
        shell,
        exchange,
        attachments,
        placed: Vec::new(),
        boxes: vec![root.clone()],
        markup: String::new(),
        lists: Vec::new(),
        code: None,
        table: None,
        classes: Vec::new(),
        dead_link: false,
    };
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for event in Parser::new_ext(markdown, options) {
        builder.event(event);
    }
    builder.flush();
    (root, builder.placed)
}
