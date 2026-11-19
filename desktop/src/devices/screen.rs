//! The devices page: each device with how the server wakes it and a
//! Forget button, and the code that pairs a phone.

use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, glib};

use super::qr;
use crate::account::{Command, Phase};
use crate::sync::client::{Device, Failure};
use crate::window::{self, Shell};

pub fn open(shell: &Rc<Shell>) {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    content.append(&window::text("Devices", &["h1"]));
    let devices = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    content.append(&devices);
    content.append(&window::text("PAIR A PHONE", &["h2"]));
    if let Phase::SignedIn { server, .. } = &shell.snapshot().phase {
        if let Some(code) = qr(server.as_str())
            && let Ok(texture) = gdk::Texture::from_bytes(&glib::Bytes::from(code.as_bytes()))
        {
            let picture = gtk::Picture::for_paintable(&texture);
            picture.set_size_request(192, 192);
            picture.set_can_shrink(true);
            picture.set_halign(gtk::Align::Start);
            picture.add_css_class("qr");
            content.append(&picture);
        }
        let address = window::text(server.as_str(), &["pair-url"]);
        address.set_selectable(true);
        content.append(&address);
    }
    content.append(&window::text(
        "Scan this code with the transom app to sign in.",
        &["muted"],
    ));
    let get = gtk::Label::builder()
        .use_markup(true)
        .label("Get the app <a href=\"https://github.com/chrisguidry/transom/releases/latest\">on GitHub</a>.")
        .xalign(0.0)
        .build();
    get.add_css_class("muted");
    content.append(&get);

    let page = window::page("Devices", &window::header(), &content);
    load(shell, &devices);
    shell.push(&page, |_| {});
}

fn load(shell: &Rc<Shell>, devices: &gtk::Box) {
    let snapshot = shell.snapshot();
    let (Some(client), Some(me)) = (snapshot.client.clone(), snapshot.me().map(str::to_owned))
    else {
        return;
    };
    let (shown, listing) = (shell.clone(), devices.clone());
    shell.run(async move { client.devices(&me).await }, move |listed| {
        list(&shown, &listing, listed);
    });
}

fn list(shell: &Rc<Shell>, devices: &gtk::Box, listed: Result<Vec<Device>, Failure>) {
    window::empty(devices);
    let mut listed = match listed {
        Ok(listed) => listed,
        Err(failure) => {
            devices.append(&window::text(&failure.to_string(), &["problem-text"]));
            return;
        }
    };
    if listed.is_empty() {
        devices.append(&window::text("No devices yet.", &["muted"]));
        return;
    }
    listed.sort_by(|one, other| (&one.name, &one.id).cmp(&(&other.name, &other.id)));
    let this = shell.places.device();
    let rows = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    rows.add_css_class("devices");
    for device in listed {
        let own = device.id == this;
        let row = gtk::Box::builder().spacing(12).build();
        row.add_css_class("entry-row");
        let text = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();
        let name = if own {
            format!("{} · this computer", device.name)
        } else {
            device.name.clone()
        };
        text.append(&window::text(&name, &[]));
        text.append(&window::text(device.wakes(), &["meta"]));
        row.append(&text);
        let forget = window::button("Forget", &["quiet"]);
        forget.set_valign(gtk::Align::Center);
        let (forgetting, listing) = (shell.clone(), devices.clone());
        forget.connect_clicked(move |_| {
            // Forgetting this computer ends its own session, which is
            // signing out.
            if own {
                forgetting.send(Command::SignOut);
                return;
            }
            let snapshot = forgetting.snapshot();
            let (Some(client), Some(me)) =
                (snapshot.client.clone(), snapshot.me().map(str::to_owned))
            else {
                return;
            };
            let (shown, listing, id) = (forgetting.clone(), listing.clone(), device.id.clone());
            forgetting.run(
                async move { client.forget(&me, &id).await },
                move |forgotten| {
                    if let Err(failure) = forgotten {
                        shown.toast(&failure.to_string());
                    }
                    load(&shown, &listing);
                },
            );
        });
        row.append(&forget);
        rows.append(
            &gtk::ListBoxRow::builder()
                .child(&row)
                .activatable(false)
                .build(),
        );
    }
    devices.append(&rows);
}
