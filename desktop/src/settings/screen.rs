//! The settings page: the server, the account and its names, this
//! computer's notifications, starting at login, when the server was last
//! heard from, the devices, and signing out.

use std::rc::Rc;

use adw::prelude::*;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde_json::json;

use super::{last_heard, start_at_login, starts_at_login};
use crate::account::{Command, Phase, Snapshot};
use crate::window::{self, Shell};

fn section(content: &gtk::Box, title: &str) {
    content.append(&window::text(title, &["h2"]));
}

fn heard_now(snapshot: &Snapshot) -> String {
    last_heard(snapshot, Timestamp::now(), &TimeZone::system())
}

pub fn open(shell: &Rc<Shell>) {
    let snapshot = shell.snapshot();
    let Phase::SignedIn { server, me } = &snapshot.phase else {
        return;
    };
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    content.append(&window::text("Settings", &["h1"]));

    section(&content, "SERVER");
    let address = window::text(server.as_str(), &[]);
    address.set_selectable(true);
    content.append(&address);

    section(&content, "ACCOUNT");
    let who = window::text(&format!("Signed in as {me}"), &[]);
    content.append(&who);
    content.append(&names(shell, me));

    section(&content, "NOTIFICATIONS");
    let notify = toggle(
        "Show notifications on this computer",
        snapshot.preferences.notifications,
    );
    notify.1.connect_active_notify({
        let shell = shell.clone();
        move |switch| {
            if shell.snapshot().preferences.notifications != switch.is_active() {
                shell.send(Command::Notifications(switch.is_active()));
            }
        }
    });
    content.append(&notify.0);
    content.append(&window::text(
        "Sounds, banners, and Do Not Disturb are in the desktop's own notification settings, under transom.",
        &["muted"],
    ));

    section(&content, "START AT LOGIN");
    let start = toggle(
        "Start transom when you log in, in the tray",
        starts_at_login(&shell.places),
    );
    start.1.connect_active_notify({
        let shell = shell.clone();
        move |switch| {
            let changed = std::env::current_exe()
                .and_then(|program| start_at_login(&shell.places, &program, switch.is_active()));
            if let Err(error) = changed {
                shell.toast(&format!("That could not be changed: {error}"));
            }
        }
    });
    content.append(&start.0);

    section(&content, "CONNECTION");
    let heard = window::text(&heard_now(&snapshot), &[]);
    content.append(&heard);

    section(&content, "DEVICES");
    let devices = window::button("Devices and pairing a phone", &[]);
    devices.set_halign(gtk::Align::Start);
    devices.connect_clicked({
        let shell = shell.clone();
        move |_| shell.open_devices()
    });
    content.append(&devices);

    section(&content, "ABOUT");
    content.append(&window::text(
        concat!(
            "transom for the desktop, version ",
            env!("CARGO_PKG_VERSION"),
            ", under the MIT license."
        ),
        &[],
    ));
    let source = gtk::Label::builder()
        .use_markup(true)
        .label(
            "<a href=\"https://github.com/chrisguidry/transom\">github.com/chrisguidry/transom</a>",
        )
        .xalign(0.0)
        .build();
    content.append(&source);

    let sign_out = window::button("Sign out", &["quiet"]);
    sign_out.set_halign(gtk::Align::Start);
    sign_out.set_margin_top(24);
    sign_out.connect_clicked({
        let shell = shell.clone();
        move |_| shell.send(Command::SignOut)
    });
    content.append(&sign_out);

    let page = window::page("Settings", &window::header(), &content);
    shell.push(&page, move |shell| {
        let snapshot = shell.snapshot();
        heard.set_text(&heard_now(&snapshot));
        if let Some(me) = snapshot.me() {
            who.set_text(&format!("Signed in as {me}"));
        }
        if notify.1.is_active() != snapshot.preferences.notifications {
            notify.1.set_active(snapshot.preferences.notifications);
        }
    });
}

/// A switch with its `label` beside it.
fn toggle(label: &str, on: bool) -> (gtk::Box, gtk::Switch) {
    let row = gtk::Box::builder().spacing(12).build();
    let text = window::text(label, &[]);
    text.set_hexpand(true);
    row.append(&text);
    let switch = gtk::Switch::builder()
        .active(on)
        .valign(gtk::Align::Center)
        .build();
    row.append(&switch);
    (row, switch)
}

/// The two names a person may change: the handle they are addressed by,
/// and the display name shown in its place.
fn names(shell: &Rc<Shell>, me: &str) -> gtk::Box {
    let rows = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    let Some(client) = shell.snapshot().client.clone() else {
        return rows;
    };

    let rename = gtk::Box::builder().spacing(8).build();
    let handle = gtk::Entry::builder()
        .placeholder_text("A new name")
        .hexpand(true)
        .build();
    let apply = window::button("Rename", &["primary"]);
    rename.append(&handle);
    rename.append(&apply);
    rows.append(&rename);
    apply.connect_clicked({
        let (shell, client, me, handle) =
            (shell.clone(), client.clone(), me.to_owned(), handle.clone());
        move |_| {
            let to = handle.text().trim().trim_start_matches('@').to_owned();
            if to.is_empty() {
                return;
            }
            let (shown, client, me, handle) =
                (shell.clone(), client.clone(), me.clone(), handle.clone());
            shell.run(
                async move { client.change(&me, json!({ "name": to })).await },
                move |renamed| match renamed {
                    Ok(identity) => {
                        handle.set_text("");
                        shown.send(Command::Renamed(format!("@{}", identity.name)));
                        shown.toast(&format!("You are now @{}", identity.name));
                    }
                    Err(failure) => shown.toast(&failure.to_string()),
                },
            );
        }
    });

    let display = gtk::Box::builder().spacing(8).build();
    let shown_as = gtk::Entry::builder()
        .placeholder_text("A display name, shown in place of your handle")
        .max_length(64)
        .hexpand(true)
        .build();
    let save = window::button("Save", &[]);
    display.append(&shown_as);
    display.append(&save);
    rows.append(&display);
    {
        let (client, me, shown_as) = (client.clone(), me.to_owned(), shown_as.clone());
        shell.run(async move { client.identity(&me).await }, move |identity| {
            if let Ok(identity) = identity
                && let Some(name) = identity.display_name
            {
                shown_as.set_text(&name);
            }
        });
    }
    save.connect_clicked({
        let (shell, me) = (shell.clone(), me.to_owned());
        move |_| {
            let Some(client) = shell.snapshot().client.clone() else {
                return;
            };
            let (shown, me, name) = (shell.clone(), me.clone(), shown_as.text().trim().to_owned());
            shell.run(
                async move { client.change(&me, json!({ "display_name": name })).await },
                move |changed| match changed {
                    Ok(_) => shown.toast("Display name saved"),
                    Err(failure) => shown.toast(&failure.to_string()),
                },
            );
        }
    });
    rows
}
