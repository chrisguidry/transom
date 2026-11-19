# 20, The Linux desktop app

Not finished. The app runs and is at parity with the web UI and the
Android app; "Not built yet" below lists what remains.

The inbox in a window on a Linux desktop, questions answered from the
desktop's notifications, and an icon in the tray, built first for GNOME
on Ubuntu.

## Decided

- The app is a Rust crate, `desktop/`, in the workspace, drawn with
  gtk4-rs and libadwaita. It uses only stock widgets, with no custom
  widget classes, so the workspace's `unsafe_code = "forbid"` holds.
- One instance runs for the whole login session. Starting the app again
  hands its arguments to the running instance and exits.
- It is at parity with the web UI and the Android app: the inbox, an
  exchange's page, a group's members, the devices, and settings. A
  screen all three should have lands in all three in the same commit.
- The window's chrome and behaviors are libadwaita's: the header bar,
  the back button, the menu, light and dark with the desktop. Inside
  the window the look is transom's, from `transom.css`: its palette,
  4 px corners, 1 px rules, and small-caps section heads. A unit test
  reads `server/src/web/static/transom.css` and fails when the app's
  colors differ.
- It signs in through the server, as the Android app does, in the
  person's own browser. The callback is `sh.transom:/callback`, the
  reverse-domain form RFC 8252 section 7.1 asks for. The app's desktop
  entry claims `x-scheme-handler/sh.transom`, so the browser hands the
  callback to a second launch of the app, which hands it to the running
  one. The server's default `app_callbacks` gains this URL. The Android
  app keeps its App Link.
- The session's token is kept in the desktop's Secret Service, in the
  default collection, under the attribute `application=sh.transom`.
- The app keeps the inbox in memory only, since it runs for the whole
  session. Replies and read marks go to an outbox file under
  `$XDG_STATE_HOME/transom/` before anything is sent, so an answer
  clicked while the network is away is never lost.
- Nothing polls. The app follows the live feed and reads the whole list
  each time the feed opens. It reconnects at once on logind's signal
  that the computer woke and NetworkManager's that the network
  connected. The one timer is the feed's backoff, a second doubling to
  a minute, while the network is up and the server does not answer.
- It registers as a device that wakes by `{"connection": true}`, named
  for the computer's hostname.
- Notifications go through a `Notifier` trait with one implementation,
  `org.freedesktop.Notifications`, which reads the server's
  capabilities and uses what it offers. On GNOME Shell 50 that is
  buttons, body markup, and pictures. A `Tray` trait has one
  implementation, `StatusNotifierItem`. A second implementation of
  either waits for a desktop that needs one.
- The app speaks D-Bus through one library, zbus. It serves the tray's
  two protocols itself, `org.kde.StatusNotifierItem` and
  `com.canonical.dbusmenu`, and makes the few Secret Service calls
  itself, in `signin/keyring.rs`, with no library for either.
- The app ships as a `.deb` in transom's GitHub releases, beside the
  APK. Plan 23 has how everything transom ships is released together. Starting
  at login is a setting that writes
  `~/.config/autostart/sh.transom.desktop`; the package adds no
  autostart entry of its own.

## Notifications on GNOME

GNOME Shell 50.1's own source, in `libshell`, settles what a
notification can do:

- A notification shows at most three buttons, and has no text box.
- A low-urgency notification shows no banner. A critical one shows
  during Do Not Disturb and stays until dismissed.
- The Shell sounds only when the notification names a sound.
- The body is one paragraph: the Shell replaces each newline with a
  space.
- An app has at most ten notifications. An eleventh removes the
  oldest, reported as expired.
- A dismissal, and "Clear" in the message list, are reported as
  dismissed.

| Urgency | On GNOME |
|---|---|
| `urgent` | Critical, with a sound, and again with each repeat. |
| `high` | A banner with a sound. |
| `normal` | A banner with a sound. |
| `low` | The message list only, with no sound. |
| `very-low` | No notification. |

The notification's title is the sender, and its body is the summary in
bold and the first line of the message. Its icon is the sender's
picture. Its buttons, three at most, follow Android's order: Got it on
an urgent message; Reply… when the question has a prompt, with its
choices beside it when they all fit; otherwise each choice, or Answer…
when they do not all fit; then the links by their titles. Pressing the
notification itself opens the exchange. Dismissing it reads the
exchange, as swiping does on Android.

A notification is withdrawn when its exchange ends, is read on any
device, or leaves the inbox. A message that replaces one whose
notification still shows takes that notification over.

The three signals a press sends, the activation token, the action, and
the close, are read from one stream in the order the Shell sent them. A
stream for each let the close overtake the press, and the press was
taken for a dismissal.

## The tray

GNOME has no tray of its own. Ubuntu's `ubuntu-appindicators` extension
hosts `StatusNotifierItem`s, and passes each click's activation token
to the item, so a window opened from the menu takes the focus.

The icon is transom's mark in the panel's color, and its attention
form, with a dot, while a question waits or something is unread. Beside
the icon is the number of questions that wait, which the item gives as
its `XAyatanaLabel`, and nothing when none does. A click opens the menu:
"Waiting on you", with each question as a submenu of its choices, then
Reply… when it takes text, then Open; answers not sent yet; Open
transom with the unread count; the notifications toggle; and Quit. A
double click or a middle click opens the window.

Each menu item keeps its id while the menu around it changes, so a
click on a menu drawn a moment ago still means the item clicked. The
item registers with `org.kde.StatusNotifierWatcher`, and again each
time a watcher takes that name, since the Shell or the extension can
start after the app or restart under it.

A tray item's menu holds plain items only. A panel of the app's own
widgets under the icon needs a GNOME Shell extension, which plan 22
covers.

## Unsent and lost replies

All three apps show these two states the same way.

- **Unsent.** A reply the outbox holds shows on its exchange in the
  muted color under "Not sent yet", and its row says "Your answer, No,
  is not sent yet". The tray's menu lists it.
- **Lost.** When the server answers a reply with `409`, someone else
  replied first. The exchange shows "Not used" over the reply, with
  "Your answer, No, was not used: Sam answered first", above the reply
  that won. A reply made from a notification or the tray also posts a
  notification that says so, since no window shows it. Leaving the
  exchange's page, or dismissing that notification, ends it.

The web UI has no outbox. A page keeps a reply whose request got no
response for as long as the page is open, shows it as unsent in place
of the reply's form, and sends it again when the person presses "Send
again". Loading the page again forgets it, and nothing sends it after
the page is closed. A lost reply made from a row of the inbox shows on
that row and in the inbox's toast. The server's `409` names who replied
first in its `reply`, which is where the page reads the name from.

The Android app keeps a lost reply in its database, so the state lasts
through a restart of the app. It posts the notification for a lost reply
made anywhere on the phone, while the exchange is unread, and marking
the exchange's row read ends the state, as leaving the exchange's screen
and dismissing the notification do.

A press on a notification in the moment after the app withdrew it still
names its exchange, so the answer is sent and, if it lost, said to be
lost.

## Testing

- The plain Rust that decides things has unit tests: the inbox's
  sections, a row's words, which notifications to show and their
  buttons, the tray's menu, the outbox, the event stream's reader, and
  sign-in's URLs.
- `test-desktop` is a workspace crate, beside `test-issuer` and
  `test-redis`, that serves the three D-Bus services the app talks to
  on whatever bus it is given: the notification server, the tray's
  registry, and the Secret Service. It records what the app sent, and a
  test presses a button or dismisses a notification through it. It also
  acts as the tray's host: a test reads the icon's properties and its
  menu, and clicks a menu item, with the D-Bus calls a host makes. Its
  `TestSystem` serves logind and NetworkManager, the two services of
  the system bus the app listens to, and sends their signals when a
  test asks. As a process, it lets the app run by hand on a private
  bus.
- `test-desktop`'s `TestBus` starts a `dbus-daemon` for one test, which
  ends with the test. Every test that speaks D-Bus runs on one, so no
  test uses the session bus or the system bus of whoever runs them. The
  code that connects to either real bus is in `main.rs`, and everything
  else takes the connection it is given.
- The app's code is tested against a real server. `desktop/src/testserver`
  starts the `transom` binary for each test, on a port the system
  chooses and under a Redis prefix of its own, with `test-issuer` as
  its issuer and the Redis of `test-redis`. It does a browser's part of
  a sign-in over plain HTTP, so no test opens a browser. Cargo builds
  the binary whenever it builds the workspace's tests.
- The account is tested end to end: a test runs it with that server and
  with `test-desktop`'s services on a `TestBus`, does what a person,
  the server, or the desktop does, and reads the window's snapshot,
  what the account asks the window to show, the server's own state, the
  desktop's notifications, and the tray's menu.
- The coverage gate holds every line of `desktop/` and `test-desktop/`
  except the GTK code. A file under `desktop/src` that builds or drives
  GTK widgets is named `main.rs`, `window.rs`, `screen.rs`, or
  `widgets.rs`, and the gate leaves out the files with those names and
  no others. What a screen computes is in its domain's other files,
  where it is tested. The regex that names those files is in
  `.pre-commit-config.yaml` and `.github/workflows/rust.yaml`.
- No test draws a widget, and no test runs a real GNOME Shell. The
  screens and the Shell are checked by hand.

## Built

- `desktop/`: sign-in, sync, the outbox, notifications, the tray, and
  every screen, by domain: `signin`, `sync`, `inbox`, `exchange`,
  `notifications`, `tray`, `groups`, `devices`, `settings`, `pictures`,
  and `theme`, with `account` as the one task that ties them together
  and `window` as the window they are drawn in.
- `test-desktop/`.
- The server's default `app_callbacks` includes `sh.transom:/callback`.
- `desktop/dev-server.sh` runs a server to sign in to, and
  `desktop/dev-install.sh` installs the build in `target/` for one
  account, with the desktop entry that claims the scheme.
- `--sign-in <address>` begins a sign-in from the command line, and
  `--background` starts without a window.
- CI for `desktop/` and `test-desktop/`: the Rust workflow runs on
  Ubuntu 26.04, with GTK, libadwaita, and `dbus-daemon` installed, and
  its coverage gate covers both crates.
- The `.deb`: `cargo deb` builds it from the metadata in
  `desktop/Cargo.toml`, with the binary, the desktop entry, and the
  icons. The Desktop workflow builds and installs it for every change to
  the app, and for a release builds the package that goes in the GitHub
  release, with a build provenance attestation. The package's version
  is the crate's with the release's tag after it, so apt takes each
  release as an upgrade. The package is built on Ubuntu 26.04 for
  amd64: the app's stylesheet needs GTK 4.16 and libadwaita 1.6.

## Not built yet

- Tests that draw the widgets. The project does no end-to-end testing
  of a GUI yet, so the coverage gate leaves the GTK files out by name
  and the screens are checked by hand.
- The disposition's badge on a notification's icon.
- A text box in the notification where the server has `inline-reply`,
  as KDE Plasma's does.
- Telling the person that a newer release exists. `/version` reports
  the web UI's own version, not a release's, so this needs another
  source.
