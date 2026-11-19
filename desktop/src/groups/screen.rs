//! A group's page: its members, each with Remove, and a box to invite
//! another.

use std::rc::Rc;

use adw::prelude::*;

use super::handle;
use crate::sync::client::Failure;
use crate::window::{self, Shell};

/// Pushes the page of `group`, a handle with its `@`.
pub fn open(shell: &Rc<Shell>, group: &str) {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    content.append(&window::text(group, &["h1"]));
    let members = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    content.append(&members);

    let invite = gtk::Box::builder().spacing(8).margin_top(18).build();
    let name = gtk::Entry::builder()
        .placeholder_text("name")
        .hexpand(true)
        .build();
    let send = window::button("Invite", &["primary"]);
    invite.append(&name);
    invite.append(&send);
    content.append(&invite);
    content.append(&window::text(
        "An invitation asks them first; they join when they accept.",
        &["muted"],
    ));

    let page = window::page(group, &window::header(), &content);
    load(shell, group, &members);
    let inviting = {
        let (shell, group, name, members) = (
            shell.clone(),
            group.to_owned(),
            name.clone(),
            members.clone(),
        );
        move || {
            let (Some(invitee), Some(client)) =
                (handle(&name.text()), shell.snapshot().client.clone())
            else {
                return;
            };
            name.set_text("");
            let (shown, group_shown, members) = (shell.clone(), group.clone(), members.clone());
            let (group, said) = (group.clone(), invitee.clone());
            shell.run(
                async move { client.invite(&group, &invitee).await },
                move |invited| match invited {
                    Ok(()) => {
                        shown.toast(&format!("Asked {said} to join {group_shown}"));
                        load(&shown, &group_shown, &members);
                    }
                    Err(failure) => shown.toast(&failure.to_string()),
                },
            );
        }
    };
    send.connect_clicked({
        let inviting = inviting.clone();
        move |_| inviting()
    });
    name.connect_activate(move |_| inviting());
    shell.push(&page, |_| {});
}

/// Reads `group`'s members from the server and lists them in `members`.
fn load(shell: &Rc<Shell>, group: &str, members: &gtk::Box) {
    let Some(client) = shell.snapshot().client.clone() else {
        return;
    };
    let (shown, listing, group) = (shell.clone(), members.clone(), group.to_owned());
    let asked = group.clone();
    shell.run(async move { client.members(&asked).await }, move |listed| {
        list(&shown, &group, &listing, listed);
    });
}

fn list(shell: &Rc<Shell>, group: &str, members: &gtk::Box, listed: Result<Vec<String>, Failure>) {
    window::empty(members);
    let names = match listed {
        Ok(names) => names,
        Err(failure) => {
            members.append(&window::text(&failure.to_string(), &["problem-text"]));
            return;
        }
    };
    for member in names {
        let row = gtk::Box::builder().spacing(12).margin_top(6).build();
        let name = window::text(&member, &[]);
        name.set_hexpand(true);
        row.append(&name);
        let remove = window::button("Remove", &["quiet"]);
        let (removing, group, listing) = (shell.clone(), group.to_owned(), members.clone());
        remove.connect_clicked(move |_| {
            let Some(client) = removing.snapshot().client.clone() else {
                return;
            };
            let (shown, group_shown, listing) = (removing.clone(), group.clone(), listing.clone());
            let (group, member) = (group.clone(), member.clone());
            removing.run(
                async move { client.remove(&group, &member).await },
                move |removed| {
                    if let Err(failure) = removed {
                        shown.toast(&failure.to_string());
                    }
                    load(&shown, &group_shown, &listing);
                },
            );
        });
        row.append(&remove);
        members.append(&row);
    }
}
