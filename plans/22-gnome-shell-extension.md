# 22, A GNOME Shell extension for the desktop app

Not built. Stub.

Plan 20's tray icon opens a menu of plain items, and anything richer
opens in a window that the compositor places. A panel of the app's own
widgets, anchored under the icon, needs an extension that runs inside
GNOME Shell.

## Decided

- The desktop app works in full without the extension. The extension
  only adds a way to reach it.

## Open

- What the panel shows that the menu and the window do not: a text box
  to reply with, the message's body, or the whole inbox.
- How the extension reads the inbox and sends replies: over D-Bus to
  the running app, which already holds the session and the outbox, or
  on its own.
- Whether the extension's icon replaces the tray icon or sits beside
  it.
- How it is distributed, on extensions.gnome.org or in the `.deb`, and
  how it keeps up with each GNOME Shell release, since an extension
  names the Shell versions it supports.
