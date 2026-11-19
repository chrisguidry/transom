//! transom's look inside the window: the palette and shapes of the web
//! UI's `transom.css`, in light and dark, over libadwaita's widgets. The
//! window's chrome and its behaviors stay the desktop's own. `widgets`
//! loads the look into GTK.

pub mod widgets;

/// One scheme's colors, named as `transom.css` names them.
#[derive(Debug, PartialEq, Eq)]
pub struct Palette {
    pub paper: &'static str,
    pub ink: &'static str,
    pub muted: &'static str,
    pub rule: &'static str,
    pub accent: &'static str,
    pub accent_ink: &'static str,
    pub soft: &'static str,
    pub fire: &'static str,
    pub good: &'static str,
    /// How clear a read row's grey picture is.
    pub faded: &'static str,
}

pub const LIGHT: Palette = Palette {
    paper: "#fbfbf9",
    ink: "#23262b",
    muted: "#6c6f75",
    rule: "#e4e3df",
    accent: "#3d5a73",
    accent_ink: "#ffffff",
    soft: "#f1f0ec",
    fire: "#c2412d",
    good: "#2f7d4f",
    faded: "0.55",
};

pub const DARK: Palette = Palette {
    paper: "#1b1c1e",
    ink: "#e6e5e2",
    muted: "#9a9b9f",
    rule: "#33353a",
    accent: "#8fb0cc",
    accent_ink: "#14171a",
    soft: "#25272b",
    fire: "#f2846f",
    good: "#7cc796",
    faded: "0.45",
};

impl Palette {
    /// The palette as CSS variables: transom's own names, and the
    /// libadwaita variables that draw the window, its header bar, and
    /// its accent, so the stock widgets take transom's colors too.
    pub fn variables(&self) -> String {
        let Self {
            paper,
            ink,
            muted,
            rule,
            accent,
            accent_ink,
            soft,
            fire,
            good,
            faded,
        } = self;
        format!(
            ":root {{\n\
             --paper: {paper}; --ink: {ink}; --muted: {muted}; --rule: {rule};\n\
             --accent: {accent}; --accent-ink: {accent_ink}; --soft: {soft};\n\
             --fire: {fire}; --good: {good}; --faded: {faded};\n\
             --window-bg-color: {paper}; --window-fg-color: {ink};\n\
             --view-bg-color: {paper}; --view-fg-color: {ink};\n\
             --headerbar-bg-color: {paper}; --headerbar-fg-color: {ink};\n\
             --headerbar-backdrop-color: {paper}; --headerbar-border-color: {rule};\n\
             --headerbar-shade-color: {rule};\n\
             --card-bg-color: {paper}; --card-fg-color: {ink};\n\
             --popover-bg-color: {paper}; --popover-fg-color: {ink};\n\
             --dialog-bg-color: {paper}; --dialog-fg-color: {ink};\n\
             --accent-bg-color: {accent}; --accent-fg-color: {accent_ink}; --accent-color: {accent};\n\
             --destructive-color: {fire}; --success-color: {good};\n\
             }}\n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colors `transom.css` gives one scheme, by their names.
    fn web(css: &str) -> Palette {
        let value = |name: &str| -> &'static str {
            let start = css
                .find(&format!("--{name}:"))
                .expect("the stylesheet names the color");
            let rest = &css[start + name.len() + 3..];
            let value = rest[..rest.find(';').unwrap()].trim().to_owned();
            Box::leak(value.into_boxed_str())
        };
        Palette {
            paper: value("paper"),
            ink: value("ink"),
            muted: value("muted"),
            rule: value("rule"),
            accent: value("accent"),
            accent_ink: value("accent-ink"),
            soft: value("soft"),
            fire: value("fire"),
            good: value("good"),
            faded: value("faded"),
        }
    }

    #[test]
    fn the_variables_give_libadwaita_transoms_colors() {
        let variables = LIGHT.variables();
        assert!(variables.starts_with(":root {\n--paper: #fbfbf9; --ink: #23262b;"));
        assert!(variables.contains("--window-bg-color: #fbfbf9; --window-fg-color: #23262b;"));
        assert!(variables.contains("--accent-bg-color: #3d5a73; --accent-fg-color: #ffffff;"));
        assert!(DARK.variables().contains("--destructive-color: #f2846f;"));
    }

    /// The app's colors are the web's, so a change to one has to be a
    /// change to both.
    #[test]
    fn the_palette_is_the_webs() {
        let css = include_str!("../../../server/src/web/static/transom.css");
        let dark = css.find("prefers-color-scheme: dark").unwrap();
        assert_eq!(web(&css[..dark]), LIGHT);
        assert_eq!(web(&css[dark..]), DARK);
    }
}
