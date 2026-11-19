//! The account run end to end: a real server, `test-desktop` as the
//! desktop's notifications, tray, and keyring on a private bus, and the
//! channels the window uses. Each test does what a person, the server,
//! or the desktop does, and reads what the outside then sees: the
//! window's snapshot, what the account asks the window to show, the
//! server's own state, the desktop's notifications, and the tray's menu.

mod answering;
mod lost;
mod session;
mod showing;

use std::sync::Arc;
use std::time::Duration;

use test_desktop::notifications::Posted;
use test_desktop::{MenuItem, TestBus, TestDesktop, TrayItem};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::{Command, Phase, Show, Snapshot, desktop};
use crate::inbox::Entry;
use crate::settings::Places;
use crate::signin::Session;
use crate::testserver::Pair;

/// How long a test waits for something that should happen at once.
const SOON: Duration = Duration::from_secs(10);

/// The account running on a desktop of its own, as `@bob` of a server
/// where `@alice` may send to him.
struct App {
    pair: Pair,
    desktop: TestDesktop,
    places: Places,
    commands: UnboundedSender<Command>,
    snapshots: watch::Receiver<Arc<Snapshot>>,
    shows: UnboundedReceiver<Show>,
    task: JoinHandle<()>,
    _bus: TestBus,
}

impl App {
    /// Starts the account with the desktop's services on its bus. What
    /// `before` does to the pair and the desktop is done first, as an
    /// earlier run of the app, or anyone else, would have.
    async fn start_after(before: impl AsyncFnOnce(&Pair, &TestDesktop)) -> Self {
        let served = async |bus: &TestBus| Some(bus.connect().await.unwrap());
        Self::start_on(served, before).await
    }

    /// As `start_after`, with what `session` gives as the account's
    /// connection to the session's bus.
    async fn start_on(
        session: impl AsyncFnOnce(&TestBus) -> Option<zbus::Connection>,
        before: impl AsyncFnOnce(&Pair, &TestDesktop),
    ) -> Self {
        let pair = Pair::start().await;
        let bus = TestBus::start();
        let desktop = TestDesktop::serve(bus.connect().await.unwrap());
        let desktop = desktop.await.unwrap();
        before(&pair, &desktop).await;
        let places = Places::fresh();
        let session = session(&bus).await;
        let (commands, heard_commands) = unbounded_channel();
        let (snapshot_sender, snapshots) = watch::channel(Arc::default());
        let (show_sender, shows) = unbounded_channel();
        let window = (heard_commands, snapshot_sender, show_sender);
        let running = desktop::run(places.clone(), session, None, window);
        Self {
            pair,
            desktop,
            places,
            commands,
            snapshots,
            shows,
            task: tokio::spawn(running),
            _bus: bus,
        }
    }

    /// The account with no session kept, once it says no one is signed
    /// in.
    async fn signed_out() -> Self {
        let mut app = Self::start_after(async |_, _| {}).await;
        app.snapshot(|snapshot| snapshot.phase != Phase::Starting)
            .await;
        app
    }

    /// The account with `@bob`'s session kept from an earlier run, once
    /// it has the server's list. What `before` does is done first.
    async fn signed_in_after(before: impl AsyncFnOnce(&Pair, &TestDesktop)) -> Self {
        let mut app = Self::start_after(async |pair, desktop| {
            keep(desktop, &pair.server.url, &pair.bob.token).await;
            before(pair, desktop).await;
        })
        .await;
        app.snapshot(|snapshot| snapshot.loaded).await;
        app
    }

    /// The account with `@bob`'s session kept from an earlier run, once
    /// it has the server's list.
    async fn signed_in() -> Self {
        Self::signed_in_after(async |_, _| {}).await
    }

    fn send(&self, command: Command) {
        self.commands.send(command).unwrap();
    }

    /// The first snapshot that `reached` is true of, from the one the
    /// window has now. It fails the test when none is in ten seconds.
    async fn snapshot(&mut self, reached: impl FnMut(&Arc<Snapshot>) -> bool) -> Arc<Snapshot> {
        let reached = tokio::time::timeout(SOON, self.snapshots.wait_for(reached));
        reached
            .await
            .expect("the snapshot is reached")
            .unwrap()
            .clone()
    }

    /// The entry of exchange `id`, once `reached` is true of it.
    async fn entry(&mut self, id: &str, reached: impl Fn(&Entry) -> bool) -> Entry {
        let reached = |snapshot: &Arc<Snapshot>| snapshot.entry(id).is_some_and(&reached);
        self.snapshot(reached).await.entry(id).unwrap().clone()
    }

    /// The entry whose summary is `summary`, once the account has one.
    async fn summarized(&mut self, summary: &str) -> Entry {
        let reached = |snapshot: &Arc<Snapshot>| has(snapshot, summary).is_some();
        let snapshot = self.snapshot(reached).await;
        has(&snapshot, summary).unwrap().clone()
    }

    /// Begins a sign-in to the pair's server from the sign-in screen,
    /// does the browser's part as `@bob`, and returns the callback the
    /// desktop would start the app with.
    async fn through_the_browser(&mut self) -> String {
        self.send(Command::SignIn(self.pair.server.url.to_string()));
        let show = self.show().await;
        let Show::Browser(login) = show else { panic!() };
        self.pair.server.browse(&login, "bob").await
    }

    /// The next thing the account asks the window to show.
    async fn show(&mut self) -> Show {
        let show = tokio::time::timeout(SOON, self.shows.recv());
        show.await.expect("the account shows something").unwrap()
    }

    /// The desktop's notification whose body has `text`, once the
    /// account's snapshot has exchange `id`. The account posts its
    /// notifications before it sends the snapshot they come from.
    async fn notification(&mut self, id: &str, text: &str) -> Posted {
        self.entry(id, |_| true).await;
        self.posted(text).expect("the notification is showing")
    }

    /// The desktop's notification whose body has `text`, if it shows
    /// one.
    fn posted(&self, text: &str) -> Option<Posted> {
        let showing = self.desktop.notifications.showing();
        showing
            .into_iter()
            .find(|posted| posted.body.contains(text))
    }

    /// The tray's icon as its host sees it.
    async fn tray(&self) -> TrayItem {
        let item = tokio::time::timeout(SOON, self.desktop.tray_item());
        item.await.expect("the tray registers").unwrap()
    }

    /// The tray's menu.
    async fn menu(&self) -> MenuItem {
        self.tray().await.menu().await.unwrap().1
    }

    /// Clicks an item of the tray's menu as a person would, after the
    /// host gave the click's token: the item labeled by the last of
    /// `path`, under the items labeled by those before it.
    async fn click(&self, path: &[&str]) {
        let tray = self.tray().await;
        let menu = tray.menu().await.unwrap().1;
        let labeled = |item: &MenuItem, label: &&str| item.find(label).unwrap().clone();
        let item = path.iter().fold(menu, |item, label| labeled(&item, label));
        tray.provide_activation_token("tray-token").await.unwrap();
        tray.click(item.id).await.unwrap();
    }

    /// The exchange `id` as the server has it for `@bob`.
    async fn on_server(&self, id: &str) -> serde_json::Value {
        self.pair.server.exchange(&self.pair.bob, id).await
    }
}

/// The entry of `snapshot` whose summary is `summary`, if it has one.
fn has<'a>(snapshot: &'a Snapshot, summary: &str) -> Option<&'a Entry> {
    let summary = Some(summary);
    let mut entries = snapshot.entries.iter();
    entries.find(|entry| entry.exchange.message.summary.as_deref() == summary)
}

/// Keeps a session of `@bob` on `server` with `token` in the desktop's
/// keyring, as an earlier run of the app would have.
async fn keep(desktop: &TestDesktop, server: &url::Url, token: &str) {
    let session = Session {
        server: server.clone(),
        token: token.to_owned(),
        me: "@bob".to_owned(),
    };
    let kept = serde_json::to_vec(&session).unwrap();
    let attributes = [("application", "sh.transom")];
    desktop.keep_secret(&attributes, &kept).await.unwrap();
}
