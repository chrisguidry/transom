//! One exchange's page: who sent it to whom, the whole message, and its
//! reply, or the choices and text box to reply with.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use jiff::tz::TimeZone;

use super::{Exchange, Reply, State, describe, markdown};
use crate::account::Command;
use crate::inbox::Entry;
use crate::pictures::widgets::avatar;
use crate::window::{self, Shell};

/// Pushes the page of exchange `id`, and reads the exchange. `reply`
/// puts the cursor in the text box.
pub fn open(shell: &Rc<Shell>, id: &str, reply: bool) {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .build();
    let page = window::page("Exchange", &window::header(), &content);
    // The text being typed outlives each redraw of the page, which comes
    // whenever anything about the exchange changes.
    let typed = gtk::TextBuffer::new(None);
    let drawn: Rc<RefCell<Option<Entry>>> = Rc::new(RefCell::new(None));
    let id = id.to_owned();
    let refresh = {
        let (id, content, typed, drawn) =
            (id.clone(), content.clone(), typed.clone(), drawn.clone());
        move |shell: &Rc<Shell>| {
            let snapshot = shell.snapshot();
            let (Some(me), entry) = (snapshot.me(), snapshot.entry(&id)) else {
                return;
            };
            if drawn.borrow().as_ref() == entry && entry.is_some() {
                return;
            }
            window::empty(&content);
            match entry {
                Some(entry) => draw(shell, &content, me, entry, &typed),
                None => content.append(&window::text(
                    if snapshot.loaded {
                        "This exchange is no longer kept."
                    } else {
                        "Loading…"
                    },
                    &["muted"],
                )),
            }
            *drawn.borrow_mut() = entry.cloned();
        }
    };
    refresh(shell);
    if shell
        .snapshot()
        .entry(&id)
        .is_some_and(|entry| entry.read == Some(false))
    {
        shell.send(Command::Mark {
            exchanges: vec![id.clone()],
            read: true,
        });
    }
    page.connect_hidden({
        let (shell, id) = (Rc::downgrade(shell), id.clone());
        move |_| {
            // Leaving the page is having seen that an answer was not used.
            if let Some(shell) = shell.upgrade()
                && shell
                    .snapshot()
                    .entry(&id)
                    .is_some_and(|entry| entry.lost.is_some())
            {
                shell.send(Command::Seen(id.clone()));
            }
        }
    });
    shell.push(&page, refresh);
    if reply {
        let content = content.clone();
        glib::idle_add_local_once(move || focus_text(&content));
    }
}

/// Puts the cursor in the page's text box, if it has one.
fn focus_text(content: &gtk::Box) {
    let mut next = content.first_child();
    while let Some(widget) = next {
        if widget.has_css_class("answer") {
            let mut inner = widget.first_child();
            while let Some(child) = inner {
                if let Some(view) = child.downcast_ref::<gtk::TextView>() {
                    view.grab_focus();
                    return;
                }
                inner = child.next_sibling();
            }
        }
        next = widget.next_sibling();
    }
}

fn draw(shell: &Rc<Shell>, content: &gtk::Box, me: &str, entry: &Entry, typed: &gtk::TextBuffer) {
    let exchange = &entry.exchange;
    let zone = TimeZone::system();
    content.append(&parties(shell, me, entry, &zone));

    let title = window::text(&describe::title(exchange), &["h1"]);
    title.set_selectable(true);
    content.append(&title);
    message(shell, content, exchange);

    if let Some(sending) = &entry.sending {
        let said = describe::said(exchange, sending.choice.as_deref(), sending.text.as_deref());
        content.append(&reply_section(
            "unsent",
            "NOT SENT YET",
            &said,
            Some("Your answer is sent when the server can be reached."),
        ));
    }
    if let (Some(mine), Some(winner)) = (&entry.lost, &exchange.reply) {
        content.append(&reply_section(
            "lost",
            "NOT USED",
            &describe::said(exchange, mine.choice.as_deref(), mine.text.as_deref()),
            Some(&describe::lost(exchange, mine, winner)),
        ));
    }
    if let Some(reply) = &exchange.reply {
        content.append(&replied(exchange, reply, me, &zone));
    } else if entry.can_reply(me) {
        content.append(&answer(shell, entry, typed));
    }
    if entry.can_withdraw(me) && exchange.state != State::Replied {
        let withdraw = window::button("Withdraw", &["quiet"]);
        withdraw.set_halign(gtk::Align::Start);
        withdraw.set_margin_top(12);
        let (withdrawing, id) = (shell.clone(), exchange.id.clone());
        withdraw.connect_clicked(move |_| withdrawing.send(Command::Withdraw(id.clone())));
        content.append(&withdraw);
    }
}

/// The sender's picture beside who sent the exchange to whom, and when.
fn parties(shell: &Rc<Shell>, me: &str, entry: &Entry, zone: &TimeZone) -> gtk::Box {
    let exchange = &entry.exchange;
    let parties = gtk::Box::builder().spacing(12).build();
    parties.append(&avatar(
        shell,
        exchange.sender(),
        exchange.from_picture.as_deref(),
        40,
        Some(exchange.disposition()),
    ));
    let meta = window::text(&describe::page_meta(entry, zone), &["meta"]);
    meta.set_hexpand(true);
    meta.set_valign(gtk::Align::Center);
    parties.append(&meta);
    // A recipient that is neither party to a two-person exchange is a
    // group this person belongs to, and its members are a page away.
    if exchange.to != me && exchange.from != me {
        let members = window::button("Members", &["plain"]);
        members.set_valign(gtk::Align::Center);
        let (opening, group) = (shell.clone(), exchange.to.clone());
        members.connect_clicked(move |_| opening.open_members(&group));
        parties.append(&members);
    }
    parties
}

/// Appends the message to `content`: its body, the attachments the body
/// does not place, and its links.
fn message(shell: &Rc<Shell>, content: &gtk::Box, exchange: &Exchange) {
    let attachments: Vec<String> = exchange
        .message
        .attachments
        .iter()
        .map(|attachment| attachment.name.clone())
        .collect();
    let body = exchange.message.body.as_deref().unwrap_or("");
    let (rendered, placed) = markdown::widgets::render(shell, body, &exchange.id, &attachments);
    if !body.trim().is_empty() {
        content.append(&rendered);
    }
    for name in attachments.iter().filter(|name| !placed.contains(name)) {
        content.append(&markdown::widgets::attachment(
            shell,
            &exchange.id,
            name,
            name,
        ));
    }
    for link in &exchange.message.links {
        let line = gtk::Label::builder()
            .use_markup(true)
            .label(format!(
                "<a href=\"{}\">{}</a>",
                glib::markup_escape_text(&link.href),
                glib::markup_escape_text(link.label())
            ))
            .xalign(0.0)
            .wrap(true)
            .build();
        content.append(&line);
    }
}

/// The exchange's reply: who replied and when, the choice, and the text.
fn replied(exchange: &Exchange, reply: &Reply, me: &str, zone: &TimeZone) -> gtk::Box {
    let section = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .build();
    section.add_css_class("reply");
    let who = if reply.by == me {
        "You".to_owned()
    } else {
        describe::party(&reply.by, reply.by_name.as_deref())
    };
    section.append(&window::text(
        &format!("{who} replied · {}", describe::in_full(reply.at, zone)),
        &["meta"],
    ));
    if let Some(choice) = &reply.choice {
        section.append(&window::text(
            &describe::said(exchange, Some(choice), None),
            &["chosen"],
        ));
    }
    if let Some(text) = &reply.text {
        let text = window::text(text, &[]);
        text.set_selectable(true);
        section.append(&text);
    }
    section
}

/// A reply of this person's that is not the exchange's reply: its
/// `state` in small capitals, what it `said`, and a `note` on why.
fn reply_section(class: &str, state: &str, said: &str, note: Option<&str>) -> gtk::Box {
    let section = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .build();
    section.add_css_class("reply");
    section.add_css_class(class);
    section.append(&window::text(state, &["state", class]));
    section.append(&window::text(said, &["chosen"]));
    if let Some(note) = note {
        section.append(&window::text(note, &["meta"]));
    }
    section
}

/// The choices and the text box. A choice sends itself, with whatever
/// text is typed; Reply sends the text with no choice, since any message
/// takes free text.
fn answer(shell: &Rc<Shell>, entry: &Entry, typed: &gtk::TextBuffer) -> gtk::Box {
    let exchange = &entry.exchange;
    let form = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .build();
    form.add_css_class("answer");
    let send = {
        let (shell, id, typed) = (shell.clone(), exchange.id.clone(), typed.clone());
        move |choice: Option<String>| {
            let text = typed
                .text(&typed.start_iter(), &typed.end_iter(), false)
                .trim()
                .to_owned();
            shell.send(Command::Reply {
                exchange: id.clone(),
                choice,
                text: (!text.is_empty()).then_some(text),
            });
            typed.set_text("");
        }
    };
    let choices = &exchange.message.choices;
    if !choices.is_empty() {
        let buttons = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .column_spacing(8)
            .row_spacing(8)
            .max_children_per_line(12)
            .homogeneous(false)
            .halign(gtk::Align::Start)
            .build();
        for choice in choices {
            let button = window::button(&choice.label, &[]);
            let (send, key) = (send.clone(), choice.key().to_owned());
            button.connect_clicked(move |_| send(Some(key.clone())));
            let child = gtk::FlowBoxChild::builder()
                .child(&button)
                .focusable(false)
                .build();
            buttons.append(&child);
        }
        form.append(&buttons);
        for choice in choices {
            let Some(description) = choice
                .description
                .as_deref()
                .filter(|text| !text.is_empty())
            else {
                continue;
            };
            let line = gtk::Label::builder()
                .use_markup(true)
                .label(format!(
                    "<b>{}</b> {}",
                    glib::markup_escape_text(&choice.label),
                    markdown::inline(description)
                ))
                .xalign(0.0)
                .wrap(true)
                .build();
            line.add_css_class("description");
            form.append(&line);
        }
    }
    let prompt = exchange.message.prompt.as_deref().unwrap_or("");
    if !prompt.is_empty() {
        form.append(&window::text(prompt, &["muted"]));
    }
    let text = gtk::TextView::builder()
        .buffer(typed)
        .wrap_mode(gtk::WrapMode::WordChar)
        .height_request(84)
        .accepts_tab(false)
        .build();
    form.append(&text);
    let reply = window::button(
        "Reply",
        &[if choices.is_empty() {
            "primary"
        } else {
            "quiet"
        }],
    );
    reply.set_halign(gtk::Align::Start);
    reply.set_sensitive(typed.char_count() > 0);
    typed.connect_changed({
        let reply = reply.downgrade();
        move |typed| {
            if let Some(reply) = reply.upgrade() {
                let text = typed.text(&typed.start_iter(), &typed.end_iter(), false);
                reply.set_sensitive(!text.trim().is_empty());
            }
        }
    });
    reply.connect_clicked(move |_| send(None));
    form.append(&reply);
    form
}
