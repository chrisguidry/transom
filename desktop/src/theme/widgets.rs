//! transom's look, loaded into GTK for every window on a display.

use super::{DARK, LIGHT};

/// The rules, the same in both schemes, after `transom.css`: 4 px
/// corners, 1 px rules, quiet buttons, small-caps section heads, and the
/// unread dot.
const RULES: &str = include_str!("transom.css");

/// Loads transom's look for every window on `display`, and follows the
/// desktop between light and dark.
pub fn load(display: &gtk::gdk::Display) {
    let rules = gtk::CssProvider::new();
    rules.load_from_string(RULES);
    gtk::style_context_add_provider_for_display(
        display,
        &rules,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let colors = gtk::CssProvider::new();
    gtk::style_context_add_provider_for_display(
        display,
        &colors,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );
    let style = adw::StyleManager::default();
    let apply = move |style: &adw::StyleManager| {
        let palette = if style.is_dark() { &DARK } else { &LIGHT };
        colors.load_from_string(&palette.variables());
    };
    apply(&style);
    style.connect_dark_notify(apply);
}
