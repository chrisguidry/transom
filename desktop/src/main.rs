//! transom for a Linux desktop: the inbox in a window, questions
//! answered from notifications, and an icon in the tray. One instance
//! runs for the whole login session; starting the app again, as the
//! desktop does with a sign-in's callback, hands the request to it.

mod account;
mod devices;
mod exchange;
mod groups;
mod inbox;
mod notifications;
mod pictures;
mod settings;
mod signin;
mod sync;
#[cfg(test)]
mod testserver;
mod theme;
mod tray;
mod window;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk::{gio, glib};
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::watch;

use account::{Command, Show, Snapshot};
use settings::{APP_ID, Places, loose_icons};

/// What the window's side of the app is built from, made before the
/// application starts and taken when it does.
struct Parts {
    places: Places,
    runtime: tokio::runtime::Handle,
    commands: tokio::sync::mpsc::UnboundedSender<Command>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
    shows: tokio::sync::mpsc::UnboundedReceiver<Show>,
}

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    // Another part of the program may have installed a provider already,
    // which is as good.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("the runtime starts");
    let places = Places::from_environment();
    let (commands, heard_commands) = unbounded_channel();
    let (snapshot_sender, snapshots) = watch::channel(Arc::new(Snapshot::default()));
    let (show_sender, shows) = unbounded_channel();
    let parts = Rc::new(RefCell::new(Some(Parts {
        places: places.clone(),
        runtime: runtime.handle().clone(),
        commands: commands.clone(),
        snapshots,
        shows,
    })));
    let background = Rc::new(RefCell::new(Some((
        heard_commands,
        snapshot_sender,
        show_sender,
    ))));

    let application = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let shell: Rc<RefCell<Option<Rc<window::Shell>>>> = Rc::new(RefCell::new(None));

    application.connect_startup({
        let (shell, runtime) = (shell.clone(), runtime.handle().clone());
        let woke = commands.clone();
        move |application| {
            let (Some(parts), Some((heard_commands, snapshot_sender, show_sender))) =
                (parts.borrow_mut().take(), background.borrow_mut().take())
            else {
                return;
            };
            if let Some(display) = gtk::gdk::Display::default() {
                theme::widgets::load(&display);
                if let Some(icons) = loose_icons(&program()) {
                    gtk::IconTheme::for_display(&display).add_search_path(icons);
                }
            }
            gtk::Window::set_default_icon_name(APP_ID);
            let places = parts.places.clone();
            runtime.spawn(async move {
                let session = zbus::Connection::session().await.ok();
                let icons = loose_icons(&program());
                let window = (heard_commands, snapshot_sender, show_sender);
                account::desktop::run(places, session, icons, window).await;
            });
            let woke = woke.clone();
            runtime.spawn(async move {
                // A computer without a system bus never hears of a wake,
                // and the feed's own backoff reconnects.
                if let Ok(system) = zbus::Connection::system().await {
                    let woke = move || {
                        let _ = woke.send(Command::Woke);
                    };
                    sync::wake::watch(system, woke).await;
                }
            });
            *shell.borrow_mut() = Some(window::build(
                application,
                parts.places,
                parts.runtime,
                parts.commands,
                parts.snapshots,
                parts.shows,
            ));
            // The app runs for the whole session with or without its
            // window, so the application is held open until Quit.
            std::mem::forget(application.hold());
        }
    });

    application.connect_command_line({
        let shell = shell.clone();
        move |_, command_line| {
            let background = heed(command_line, &commands);
            if !background && let Some(shell) = shell.borrow().as_ref() {
                shell.present(None);
            }
            glib::ExitCode::SUCCESS
        }
    });

    let code = application.run();
    runtime.shutdown_background();
    code
}

/// Sends the account what a command line asks of it, and returns whether
/// the command line asks for no window.
fn heed(
    command_line: &gio::ApplicationCommandLine,
    commands: &tokio::sync::mpsc::UnboundedSender<Command>,
) -> bool {
    let arguments: Vec<String> = command_line
        .arguments()
        .iter()
        .skip(1)
        .filter_map(|argument| argument.to_str().map(str::to_owned))
        .collect();
    let (asked, background) = account::arguments::heed(&arguments);
    for command in asked {
        let _ = commands.send(command);
    }
    background
}

/// The program's own file, or an empty path when the system does not say.
fn program() -> PathBuf {
    std::env::current_exe().unwrap_or_default()
}
