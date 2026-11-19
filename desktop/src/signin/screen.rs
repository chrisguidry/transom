//! The sign-in screen: the server's address, and a button that sends the
//! person's browser to sign in there.

use std::rc::Rc;

use adw::prelude::*;

use crate::account::{Command, Phase};
use crate::window::{self, Shell};

/// Builds the screen, and returns it with the way to draw it again from
/// the account's snapshot.
pub fn build(shell: &Rc<Shell>) -> (adw::ToolbarView, impl Fn(&Rc<Shell>) + 'static) {
    let heading = window::text("Sign in to transom", &["h1"]);
    let explain = window::text(
        "Enter your transom server's address. Your browser opens its sign-in page, and brings you back here.",
        &["muted"],
    );
    let address = gtk::Entry::builder()
        .placeholder_text("transom.example.com")
        .input_purpose(gtk::InputPurpose::Url)
        .activates_default(true)
        .build();
    if let Some(server) = &shell.snapshot().preferences.server {
        address.set_text(server);
    }
    let sign_in = window::button("Sign in", &["primary"]);
    sign_in.set_halign(gtk::Align::Start);
    let problem = window::text("", &["problem-text"]);
    let waiting = window::text(
        "Finish signing in in your browser. This window continues when you do.",
        &["muted"],
    );
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .valign(gtk::Align::Center)
        .build();
    for child in [
        heading.upcast_ref::<gtk::Widget>(),
        explain.upcast_ref(),
        address.upcast_ref(),
        sign_in.upcast_ref(),
        problem.upcast_ref(),
        waiting.upcast_ref(),
    ] {
        content.append(child);
    }
    let column = adw::Clamp::builder()
        .maximum_size(420)
        .child(&content)
        .build();
    column.add_css_class("page");
    let view = adw::ToolbarView::new();
    view.add_top_bar(&window::header());
    view.set_content(Some(&column));

    let begin = {
        let (shell, address) = (shell.clone(), address.clone());
        move || shell.send(Command::SignIn(address.text().to_string()))
    };
    sign_in.connect_clicked({
        let begin = begin.clone();
        move |_| begin()
    });
    address.connect_activate(move |_| begin());

    let refresh = move |shell: &Rc<Shell>| {
        let snapshot = shell.snapshot();
        let Phase::SignedOut {
            problem: said,
            waiting: under_way,
        } = &snapshot.phase
        else {
            return;
        };
        problem.set_text(said.as_deref().unwrap_or(""));
        problem.set_visible(said.is_some());
        waiting.set_visible(*under_way);
        if address.text().is_empty()
            && let Some(server) = &snapshot.preferences.server
        {
            address.set_text(server);
        }
    };
    (view, refresh)
}
