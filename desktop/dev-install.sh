#!/usr/bin/env bash
# Installs this checkout's build of the desktop app for your own account,
# so you can use it while you work on it: the desktop entry, which also
# claims the sh.transom scheme that sign-in comes back through, and the
# icons, under ~/.local/share. The entry runs the binary in this
# checkout's target/, so a rebuild is all a change needs.
#
#   ./dev-install.sh             # build and install
#   ./dev-install.sh --remove    # take it out again
set -euo pipefail
cd "$(dirname "$0")"

data="${XDG_DATA_HOME:-$HOME/.local/share}"
entry="$data/applications/sh.transom.desktop"
icons="$data/icons/hicolor"

if [[ "${1:-}" == "--remove" ]]; then
    rm -f "$entry" \
        "$icons/scalable/apps/sh.transom.svg" \
        "$icons/symbolic/apps/sh.transom-symbolic.svg" \
        "$icons/symbolic/apps/sh.transom-attention-symbolic.svg" \
        "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/sh.transom.desktop"
    update-desktop-database "$data/applications"
    echo "Removed. The app's own files are under ~/.local/state/transom, ~/.config/transom, and ~/.cache/transom."
    exit 0
fi

cargo build --package transom-desktop
program="$(cd .. && pwd)/target/debug/transom-desktop"

mkdir -p "$data/applications" "$icons/scalable/apps" "$icons/symbolic/apps"
sed "s|^Exec=transom-desktop|Exec=$program|" data/sh.transom.desktop >"$entry"
cp data/icons/hicolor/scalable/apps/sh.transom.svg "$icons/scalable/apps/"
cp data/icons/hicolor/symbolic/apps/sh.transom-*.svg "$icons/symbolic/apps/"
update-desktop-database "$data/applications"
xdg-mime default sh.transom.desktop x-scheme-handler/sh.transom
gtk-update-icon-cache --quiet --ignore-theme-index "$icons" 2>/dev/null || true

echo "Installed. Start transom from the app grid, or run: $program"
