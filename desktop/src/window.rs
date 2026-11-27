//! The window: libadwaita's chrome around transom's screens. It shows
//! the sign-in screen while no one is signed in, and otherwise the inbox,
//! with each exchange, the devices, a group's members, and the settings
//! as pages pushed over it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk::{gio, glib};
use tokio::sync::mpsc::UnboundedSender;

use crate::account::{Command, Phase, Show, Snapshot};
use crate::settings::Places;
use crate::{devices, exchange, groups, inbox, settings, signin};

type Refresh = Rc<dyn Fn(&Rc<Shell>)>;

/// What every screen is built with: the window's parts, the account's
/// latest snapshot, and the way to ask the account for something.
pub struct Shell {
    pub window: adw::ApplicationWindow,
    pub navigation: adw::NavigationView,
    pub places: Places,
    toasts: adw::ToastOverlay,
    stack: gtk::Stack,
    runtime: tokio::runtime::Handle,
    commands: UnboundedSender<Command>,
    snapshot: RefCell<Arc<Snapshot>>,
    /// Each page's way to draw itself again, kept while its page lives.
    refreshers: RefCell<Vec<(glib::WeakRef<adw::NavigationPage>, Refresh)>>,
    /// Each picture already decoded, by its URL. A list draws every row
    /// again when anything in it changes, and decoding a picture for
    /// each row each time would hold up the window.
    pub textures: RefCell<HashMap<String, gtk::gdk::Texture>>,
}

impl Shell {
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot.borrow().clone()
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Runs `future` off the window's thread, and `then` on it with what
    /// the future made.
    pub fn run<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
        then: impl FnOnce(T) + 'static,
    ) {
        let task = self.runtime.spawn(future);
        glib::spawn_future_local(async move {
            if let Ok(value) = task.await {
                then(value);
            }
        });
    }

    /// Says `text` once, at the foot of the window.
    pub fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(text));
    }

    /// Says `text` with an Undo button that calls `undo`.
    pub fn toast_with_undo(&self, text: &str, undo: impl Fn() + 'static) {
        let toast = adw::Toast::builder()
            .title(text)
            .button_label("Undo")
            .timeout(6)
            .build();
        toast.connect_button_clicked(move |_| undo());
        self.toasts.add_toast(toast);
    }

    /// Pushes `page` over the others, and calls `refresh` each time the
    /// account's snapshot changes while the page is there.
    pub fn push(
        self: &Rc<Self>,
        page: &adw::NavigationPage,
        refresh: impl Fn(&Rc<Shell>) + 'static,
    ) {
        self.watch(page, refresh);
        self.navigation.push(page);
    }

    fn watch(&self, page: &adw::NavigationPage, refresh: impl Fn(&Rc<Shell>) + 'static) {
        self.refreshers
            .borrow_mut()
            .push((page.downgrade(), Rc::new(refresh)));
    }

    pub fn open_exchange(self: &Rc<Self>, id: &str, reply: bool) {
        exchange::screen::open(self, id, reply);
    }

    pub fn open_members(self: &Rc<Self>, group: &str) {
        groups::screen::open(self, group);
    }

    pub fn open_devices(self: &Rc<Self>) {
        devices::screen::open(self);
    }

    pub fn open_settings(self: &Rc<Self>) {
        settings::screen::open(self);
    }

    /// Opens `address` in the person's browser. The window is the
    /// browser's parent only while it shows: GTK asks the compositor for
    /// a handle to the parent, and a window closed to the tray has no
    /// toplevel left to export, so the compositor answers with a protocol
    /// error and disconnects the app. That is the case for a link pressed
    /// on a notification.
    pub fn browse(&self, address: &str) {
        let parent = self.window.is_visible().then_some(&self.window);
        gtk::UriLauncher::new(address).launch(parent, gio::Cancellable::NONE, |_| {});
    }

    /// Brings the window to the front. `token` is what the desktop gave
    /// for the click that asked for it; without one, the desktop may only
    /// flag the window instead of focusing it.
    pub fn present(&self, token: Option<&str>) {
        if let Some(token) = token {
            self.window.set_startup_id(token);
        }
        self.window.present();
    }

    fn update(self: &Rc<Self>, snapshot: Arc<Snapshot>) {
        let page = match snapshot.phase {
            Phase::Starting => "starting",
            Phase::SignedOut { .. } => "signin",
            Phase::SignedIn { .. } => "main",
        };
        if page != "main" {
            self.navigation.pop_to_tag("inbox");
        }
        self.stack.set_visible_child_name(page);
        *self.snapshot.borrow_mut() = snapshot;
        let refreshers: Vec<Refresh> = {
            let mut refreshers = self.refreshers.borrow_mut();
            refreshers
                .retain(|(page, _)| page.upgrade().is_some_and(|page| page.parent().is_some()));
            refreshers
                .iter()
                .map(|(_, refresh)| refresh.clone())
                .collect()
        };
        for refresh in refreshers {
            refresh(self);
        }
    }

    fn show(self: &Rc<Self>, show: Show) {
        match show {
            Show::Inbox { token } => {
                self.navigation.pop_to_tag("inbox");
                self.present(token.as_deref());
            }
            Show::Exchange { id, token, reply } => {
                self.navigation.pop_to_tag("inbox");
                self.open_exchange(&id, reply);
                self.present(token.as_deref());
            }
            Show::Browser(address) => self.browse(&address),
            Show::Notice(text) => self.toast(&text),
            Show::Quit => {
                if let Some(application) = self.window.application() {
                    application.quit();
                }
            }
        }
    }
}

/// Builds the window, hidden, and keeps it in step with the account.
pub fn build(
    application: &adw::Application,
    places: Places,
    runtime: tokio::runtime::Handle,
    commands: UnboundedSender<Command>,
    mut snapshots: tokio::sync::watch::Receiver<Arc<Snapshot>>,
    mut shows: tokio::sync::mpsc::UnboundedReceiver<Show>,
) -> Rc<Shell> {
    let navigation = adw::NavigationView::new();
    let stack = gtk::Stack::new();
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&stack));
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("transom")
        .default_width(560)
        .default_height(760)
        .hide_on_close(true)
        .content(&toasts)
        .build();
    window.add_css_class("transom");
    let shell = Rc::new(Shell {
        window,
        navigation,
        places,
        toasts,
        stack,
        runtime,
        commands,
        snapshot: RefCell::new(snapshots.borrow().clone()),
        refreshers: RefCell::new(Vec::new()),
        textures: RefCell::new(HashMap::new()),
    });

    let starting = adw::ToolbarView::new();
    starting.add_top_bar(&adw::HeaderBar::new());
    starting.set_content(Some(
        &adw::Spinner::builder()
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .width_request(32)
            .height_request(32)
            .build(),
    ));
    shell.stack.add_named(&starting, Some("starting"));
    let (signin_page, refresh_signin) = signin::screen::build(&shell);
    shell.stack.add_named(&signin_page, Some("signin"));
    shell.stack.add_named(&shell.navigation, Some("main"));
    let (inbox_page, refresh_inbox) = inbox::screen::build(&shell);
    inbox_page.set_tag(Some("inbox"));
    shell.navigation.add(&inbox_page);
    shell.watch(&inbox_page, move |shell| {
        refresh_inbox(shell);
        refresh_signin(shell);
    });

    let updating = shell.clone();
    glib::spawn_future_local(async move {
        loop {
            let snapshot = snapshots.borrow_and_update().clone();
            updating.update(snapshot);
            if snapshots.changed().await.is_err() {
                return;
            }
        }
    });
    let showing = shell.clone();
    glib::spawn_future_local(async move {
        while let Some(show) = shows.recv().await {
            showing.show(show);
        }
    });
    shell
}

/// A page's header: the wordmark, with libadwaita's own back button on
/// every page but the first.
pub fn header() -> adw::HeaderBar {
    let wordmark = gtk::Label::new(Some("transom"));
    wordmark.add_css_class("wordmark");
    adw::HeaderBar::builder().title_widget(&wordmark).build()
}

/// A page whose `content` scrolls in a column 42 rem wide at most, as
/// the web's `main` is, under `header`.
pub fn page(
    title: &str,
    header: &adw::HeaderBar,
    content: &impl IsA<gtk::Widget>,
) -> adw::NavigationPage {
    let column = adw::Clamp::builder()
        .maximum_size(672)
        .child(content)
        .build();
    column.add_css_class("page");
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&column)
        .build();
    let view = adw::ToolbarView::new();
    view.add_top_bar(header);
    view.set_content(Some(&scrolled));
    adw::NavigationPage::builder()
        .title(title)
        .child(&view)
        .build()
}

/// A line of text that wraps, starts at the left, and takes `classes`.
pub fn text(text: &str, classes: &[&str]) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .build();
    for class in classes {
        label.add_css_class(class);
    }
    label
}

/// A button labelled `label`, with `classes`.
pub fn button(label: &str, classes: &[&str]) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    for class in classes {
        button.add_css_class(class);
    }
    button
}

/// Takes every child out of `container`.
pub fn empty(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}
