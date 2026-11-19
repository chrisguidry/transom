//! The inbox, the window's first page: what waits on this person, then
//! what they have not read, then everything else, as the web's inbox
//! lists them.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use jiff::Timestamp;
use jiff::tz::TimeZone;

use super::{Drawn, Entry, Listing};
use crate::account::Command;
use crate::exchange::describe;
use crate::pictures::widgets::avatar;
use crate::window::{self, Shell};

/// Builds the page, and returns it with the way to draw it again from
/// the account's snapshot.
pub fn build(shell: &Rc<Shell>) -> (adw::NavigationPage, impl Fn(&Rc<Shell>) + 'static) {
    let (page, refresh) = build_page(shell);
    let refresh = Rc::new(refresh);
    // The rows say how long ago each arrived, so the page draws again as
    // the minutes pass, while the window is on screen.
    glib::timeout_add_seconds_local(30, {
        let (shell, refresh) = (Rc::downgrade(shell), refresh.clone());
        move || {
            let Some(shell) = shell.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if shell.window.is_visible() {
                refresh(&shell);
            }
            glib::ControlFlow::Continue
        }
    });
    (page, move |shell: &Rc<Shell>| refresh(shell))
}

fn build_page(shell: &Rc<Shell>) -> (adw::NavigationPage, impl Fn(&Rc<Shell>) + 'static) {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    let header = window::header();
    let who = gtk::Label::new(None);
    who.add_css_class("who");
    let bell = gtk::ToggleButton::builder()
        .tooltip_text("Notifications on this computer")
        .build();
    bell.add_css_class("flat");
    let menu = gio::Menu::new();
    menu.append(Some("Devices"), Some("win.devices"));
    menu.append(Some("Settings"), Some("win.settings"));
    menu.append(Some("Sign out"), Some("win.sign-out"));
    let more = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text("Menu")
        .build();
    header.pack_end(&more);
    header.pack_end(&bell);
    header.pack_end(&who);
    actions(shell);
    bell.connect_clicked({
        let shell = shell.clone();
        move |bell| shell.send(Command::Notifications(bell.is_active()))
    });

    let page = window::page("Inbox", &header, &content);
    let drawn: RefCell<Option<Drawn>> = RefCell::new(None);
    let refresh = move |shell: &Rc<Shell>| {
        let snapshot = shell.snapshot();
        let Some(me) = snapshot.me() else {
            *drawn.borrow_mut() = None;
            return;
        };
        let on = snapshot.preferences.notifications;
        bell.set_active(on);
        bell.set_icon_name(if on {
            "preferences-system-notifications-symbolic"
        } else {
            "notifications-disabled-symbolic"
        });
        who.set_text(me);
        let now = Timestamp::now();
        let next = Drawn::of(me, &snapshot, now);
        if drawn.borrow().as_ref() == Some(&next) {
            return;
        }
        draw(shell, &content, &next, now);
        *drawn.borrow_mut() = Some(next);
    };
    (page, refresh)
}

/// The window's menu: the pages it opens, and signing out.
fn actions(shell: &Rc<Shell>) {
    action(shell, "devices", Shell::open_devices);
    action(shell, "settings", Shell::open_settings);
    action(shell, "sign-out", |shell| shell.send(Command::SignOut));
}

/// Adds the action `name` to the window, which does `run`.
fn action(shell: &Rc<Shell>, name: &str, run: impl Fn(&Rc<Shell>) + 'static) {
    let action = gio::SimpleAction::new(name, None);
    let acting = Rc::downgrade(shell);
    action.connect_activate(move |_, _| {
        if let Some(shell) = acting.upgrade() {
            run(&shell);
        }
    });
    shell.window.add_action(&action);
}

fn draw(shell: &Rc<Shell>, content: &gtk::Box, drawn: &Drawn, now: Timestamp) {
    window::empty(content);
    let Drawn { me, inbox, .. } = drawn;
    if !drawn.connected {
        let offline = window::text(
            if drawn.loaded {
                "Not connected to the server. What you do here is sent when it is back."
            } else {
                "Connecting to the server…"
            },
            &["banner-quiet"],
        );
        content.append(&offline);
    }
    content.append(&window::text(me, &["h1"]));
    if !drawn.loaded {
        return;
    }
    if inbox.is_empty() {
        content.append(&window::text(
            &format!("Nothing here yet. Messages sent to {me} show up here."),
            &["muted"],
        ));
    } else if inbox.waiting.is_empty() && inbox.unread.is_empty() {
        content.append(&window::text("You're all caught up.", &["muted"]));
    }
    if !inbox.waiting.is_empty() {
        content.append(&heading(&format!(
            "WAITING ON YOU · {}",
            inbox.waiting.len()
        )));
        let rows: Vec<Listing> = inbox
            .waiting
            .iter()
            .map(|entry| Listing {
                entries: vec![entry.clone()],
            })
            .collect();
        content.append(&list(shell, me, &rows, true, now));
    }
    if !inbox.unread.is_empty() {
        let head = gtk::Box::builder().spacing(12).build();
        let title = heading(&format!("UNREAD · {}", inbox.unread.len()));
        title.set_hexpand(true);
        head.append(&title);
        let mark_all = window::button("Mark all read", &["plain"]);
        mark_all.set_valign(gtk::Align::End);
        let (marking, ids) = (shell.clone(), inbox.markable.clone());
        mark_all.connect_clicked(move |_| mark_read(&marking, &ids));
        head.append(&mark_all);
        content.append(&head);
        content.append(&list(shell, me, &inbox.unread, true, now));
    }
    if !inbox.earlier.is_empty() {
        content.append(&heading("EARLIER"));
        content.append(&list(shell, me, &inbox.earlier, false, now));
    }
    if !inbox.older.is_empty() {
        let older = gtk::Expander::builder()
            .label(format!("Show {} older", inbox.older.len()))
            .child(&list(shell, me, &inbox.older, false, now))
            .margin_top(12)
            .build();
        older.add_css_class("muted");
        content.append(&older);
    }
}

fn heading(text: &str) -> gtk::Label {
    window::text(text, &["h2"])
}

/// Marks `ids` read, and offers to undo it for a few seconds.
fn mark_read(shell: &Rc<Shell>, ids: &[String]) {
    shell.send(Command::Mark {
        exchanges: ids.to_vec(),
        read: true,
    });
    let said = if ids.len() == 1 {
        "Marked read".to_owned()
    } else {
        format!("Marked {} read", ids.len())
    };
    let (undoing, ids) = (shell.clone(), ids.to_vec());
    shell.toast_with_undo(&said, move || {
        undoing.send(Command::Mark {
            exchanges: ids.clone(),
            read: false,
        });
    });
}

fn list(shell: &Rc<Shell>, me: &str, rows: &[Listing], full: bool, now: Timestamp) -> gtk::ListBox {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    list.add_css_class("exchanges");
    for listing in rows {
        list.append(&row(shell, me, listing, full, now));
    }
    let (opening, ids): (Rc<Shell>, Vec<String>) = (
        shell.clone(),
        rows.iter()
            .map(|listing| listing.entry().exchange.id.clone())
            .collect(),
    );
    list.connect_row_activated(move |_, row| {
        if let Some(id) = usize::try_from(row.index())
            .ok()
            .and_then(|index| ids.get(index))
        {
            opening.open_exchange(id, false);
        }
    });
    list
}

/// A row: the picture, then the title with its time, the first line of
/// the body, and who it is from. Its answers sit under it, and Mark read
/// beside it.
fn row(
    shell: &Rc<Shell>,
    me: &str,
    listing: &Listing,
    full: bool,
    now: Timestamp,
) -> gtk::ListBoxRow {
    let entry: &Entry = listing.entry();
    let exchange = &entry.exchange;
    let unread = entry.read == Some(false);
    let card = gtk::Box::builder().spacing(12).build();
    card.add_css_class("entry-row");
    card.add_css_class(if full { "full" } else { "read" });
    let sender = if exchange.from == me {
        exchange.to_name.as_deref().unwrap_or(&exchange.to)
    } else {
        exchange.sender()
    };
    let picture = if exchange.from == me {
        exchange.to_picture.as_deref()
    } else {
        exchange.from_picture.as_deref()
    };
    card.append(&avatar(
        shell,
        sender,
        picture,
        if full { 40 } else { 32 },
        Some(exchange.disposition()),
    ));
    card.append(&words(entry, me, full, now));

    if unread {
        let mark = window::button("Mark read", &["quiet", "mark-read"]);
        mark.set_valign(gtk::Align::Center);
        let (marking, ids) = (shell.clone(), listing.ids());
        mark.connect_clicked(move |_| mark_read(&marking, &ids));
        card.append(&mark);
    }

    let whole = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    whole.append(&card);
    let choices = entry.row_choices(me);
    if !choices.is_empty() {
        let answers = gtk::Box::builder().spacing(8).build();
        answers.add_css_class("row-choices");
        for choice in choices {
            let answer = window::button(&choice.label, &[]);
            let (answering, id, key) =
                (shell.clone(), exchange.id.clone(), choice.key().to_owned());
            answer.connect_clicked(move |_| {
                answering.send(Command::Reply {
                    exchange: id.clone(),
                    choice: Some(key.clone()),
                    text: None,
                });
            });
            answers.append(&answer);
        }
        whole.append(&answers);
    }
    gtk::ListBoxRow::builder().child(&whole).build()
}

/// A row's words: the title with its time, the first line of the body
/// when the row is `full`, and who it is from.
fn words(entry: &Entry, me: &str, full: bool, now: Timestamp) -> gtk::Box {
    let exchange = &entry.exchange;
    let text = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .hexpand(true)
        .build();
    let line = gtk::Box::builder().spacing(8).build();
    if entry.read == Some(false) {
        let dot = gtk::Box::builder().valign(gtk::Align::Center).build();
        dot.add_css_class("unread-dot");
        dot.set_tooltip_text(Some("Unread"));
        line.append(&dot);
    }
    let summary = gtk::Label::builder()
        .label(describe::title(exchange))
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    summary.add_css_class("summary");
    line.append(&summary);
    let zone = TimeZone::system();
    let when = gtk::Label::new(Some(&describe::ago(exchange.created, now, &zone)));
    when.add_css_class("when");
    when.set_tooltip_text(Some(&describe::in_full(exchange.created, &zone)));
    line.append(&when);
    text.append(&line);
    let excerpt = describe::excerpt(exchange);
    if full && !excerpt.is_empty() {
        let excerpt = gtk::Label::builder()
            .label(excerpt)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        excerpt.add_css_class("excerpt");
        text.append(&excerpt);
    }
    let meta = gtk::Label::builder()
        .label(describe::meta(entry, me))
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    meta.add_css_class("meta");
    if entry.sending.is_some() {
        meta.add_css_class("unsent");
    }
    if entry.lost.is_some() {
        meta.add_css_class("lost");
    }
    text.append(&meta);
    text
}
