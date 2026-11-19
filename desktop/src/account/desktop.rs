//! The account on a desktop: its notifications, tray, and keyring are the
//! services of the session's bus, and it runs without any the desktop
//! does not have.

use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::watch;

use super::{Account, Command, Hearing, Show, Snapshot};
use crate::notifications::freedesktop::Freedesktop;
use crate::settings::Places;
use crate::signin::keyring::Keyring;
use crate::tray::status_notifier::StatusNotifier;

/// The account's ends of the channels between it and the window: the
/// commands the window sends, the snapshots the window draws, and what
/// the account asks the window to show.
pub type Window = (
    UnboundedReceiver<Command>,
    watch::Sender<Arc<Snapshot>>,
    UnboundedSender<Show>,
);

/// Connects the account to the notifications, tray, and keyring of
/// `session`, the session's bus, and runs it until the person quits.
/// `icons` is the directory of the app's icons when it is not installed.
pub async fn run(
    places: Places,
    session: Option<zbus::Connection>,
    icons: Option<&str>,
    (commands, snapshots, shows): Window,
) {
    let (notified_sender, notified) = unbounded_channel();
    let (picked_sender, picked) = unbounded_channel();
    // A tray host looks in the directory an item names for the icon's
    // own file, without a theme's layout of subdirectories.
    let icons = icons
        .map(|icons| format!("{icons}/hicolor/symbolic/apps"))
        .unwrap_or_default();
    let (mut freedesktop, mut tray, mut keyring) = (None, None, None);
    if let Some(session) = session {
        freedesktop = Freedesktop::open(&session, &places.state, notified_sender)
            .await
            .inspect_err(|error| tracing::warn!(%error, "no notification service was found"))
            .ok();
        tray = StatusNotifier::serve(session.clone(), picked_sender, icons)
            .await
            .inspect_err(|error| tracing::warn!(%error, "no tray was found"))
            .ok();
        keyring = Some(Keyring::new(session));
    }
    let hearing = Hearing {
        commands,
        notified,
        picked,
    };
    Account::run(
        places,
        keyring,
        freedesktop,
        tray,
        snapshots,
        shows,
        hearing,
    )
    .await;
}
