# The Linux desktop app

The inbox in a window, questions answered from the desktop's
notifications, and an icon in the tray. [Plan 20](../plans/20-desktop-app.md)
is its design.

## Installing it

Each push to `main` that changes the app publishes a GitHub release
whose tag begins `desktop-`, with `transom-desktop_<time>-<commit>_amd64.deb`
and `SHA256SUMS`. The package is built on Ubuntu 26.04 and needs GTK
4.16 and libadwaita 1.6 or later. Download both files from the newest
such release, check them, and install:

```
gh attestation verify transom-desktop_<time>-<commit>_amd64.deb --repo chrisguidry/transom
sha256sum --check SHA256SUMS
sudo apt install ./transom-desktop_<time>-<commit>_amd64.deb
```

The attestation shows that this repository's Desktop workflow built the
package, and from which commit. Each release's version is higher than
the one before, so installing a newer package upgrades the app. Start
transom from the app grid, sign in to your server, and turn on "Start
transom when you log in" in its settings.

## Building it

The build needs the Rust toolchain that `../rust-toolchain.toml` names,
and the development files of GTK 4 and libadwaita. On Ubuntu:

```
sudo apt install libgtk-4-dev libadwaita-1-dev
```

From the repository's root:

```
cargo build --package transom-desktop
cargo test
```

The app's tests run it against a real server, the `transom` binary, and
against `test-desktop`'s notification server, tray host, and keyring on
a message bus of their own, so they need `dbus-daemon` and, for the
Redis that `test-redis` starts, Docker. `cargo test` over the whole
workspace builds the server before it runs them. To run this crate's
tests alone, build the server first:

```
cargo build --package transom
cargo test --package transom-desktop --package test-desktop
```

The repository's coverage gate holds every line of this crate except
the GTK code, which no test draws yet: a file under `src/` that builds
or drives GTK widgets is named `main.rs`, `window.rs`, `screen.rs`, or
`widgets.rs`, and the gate leaves out the files with those names and no
others. What a screen computes belongs in its domain's other files,
with a test.

## What it needs from the desktop

Each of these is found on the session bus when the app starts, and the
app runs without any that is missing:

- A notification server, `org.freedesktop.Notifications`. Every common
  desktop has one.
- A tray host for `StatusNotifierItem`. KDE Plasma has one. GNOME needs
  an extension: Ubuntu ships `ubuntu-appindicators` turned on, and other
  distributions have "AppIndicator and KStatusNotifierItem Support".
- A Secret Service, such as GNOME Keyring, to keep the session between
  runs. Without one, each start signs in again.

## What it needs from the server

The server sends the browser back to the app at `sh.transom:/callback`
when a sign-in finishes. A server from a release that includes the
desktop app allows that callback by default. An older server allows it
when its `app_callbacks` setting names it beside the Android app's:

```
TRANSOM_APP_CALLBACKS='https://transom.sh/app/callback sh.transom:/callback'
```

## Running it while you work on it

The desktop starts the app with the callback's URL, so the app's desktop
entry has to be installed for sign-in to finish. `dev-install.sh` builds
the app and installs the entry and the icons under `~/.local/share`,
pointing at the binary in `../target/debug/`:

```
./dev-install.sh             # build and install for your account
./dev-install.sh --remove    # take it out again
```

`dev-server.sh` runs a transom to sign in to, at
`http://127.0.0.1:7800`, with `test-issuer` as its issuer and a Redis
that `test-redis` starts in Docker. It prints how to send a message as
someone else:

```
./dev-server.sh              # the browser signs in as @chris
```

The issuer signs in whoever asks, with no password, so it is for a
developer's own machine only.

The app's arguments:

```
transom-desktop                        # open the window
transom-desktop --background           # the tray and notifications, no window
transom-desktop --sign-in <address>    # begin signing in to a server
```

One instance runs for the session. A second start hands its arguments
to the first and exits.

## Where it keeps things

| What | Where |
|---|---|
| The session's token | The Secret Service's default collection, under `application=sh.transom` |
| Replies and read marks not sent yet | `$XDG_STATE_HOME/transom/outbox.json` |
| This computer's device id | `$XDG_STATE_HOME/transom/device` |
| The notifications it is showing | `$XDG_STATE_HOME/transom/notified.json` |
| Preferences | `$XDG_CONFIG_HOME/transom/settings.json` |
| Pictures from the server | `$XDG_CACHE_HOME/transom/pictures/` |
| Starting at login | `$XDG_CONFIG_HOME/autostart/sh.transom.desktop` |

Signing out forgets the token, the outbox, and the pictures, and keeps
the device id, so signing in again registers the same device.

## Testing against a desktop that is not yours

`test-desktop` serves a notification server, a tray registry, and a
Secret Service on whatever session bus it finds, so the app can run on a
private bus where nothing reaches your own desktop:

```
dbus-run-session -- sh -c '
  ../target/debug/test-desktop &
  ../target/debug/transom-desktop'
```

`sh.transom.TestDesktop`, at `/sh/transom/TestDesktop` on that bus,
lists the notifications as JSON, and presses or dismisses one:

```
gdbus call --session --dest sh.transom.TestDesktop \
    --object-path /sh/transom/TestDesktop \
    --method sh.transom.TestDesktop.Notifications
gdbus call --session --dest sh.transom.TestDesktop \
    --object-path /sh/transom/TestDesktop \
    --method sh.transom.TestDesktop.Invoke 1 'choice:Restart'
```
