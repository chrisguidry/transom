//! What the app keeps on this computer besides its session: where its
//! files go, the person's few preferences, and whether the desktop starts
//! it at login.

pub mod screen;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};

use crate::account::Snapshot;
use crate::exchange::describe;

/// The app's id: its desktop entry, its icon, and its bus name.
pub const APP_ID: &str = "sh.transom";

/// The directories the app writes under, by the XDG base directory
/// rules.
#[derive(Clone, Debug)]
pub struct Places {
    /// The outbox, this computer's device id, and what was notified.
    pub state: PathBuf,
    /// The preferences.
    pub config: PathBuf,
    /// Pictures fetched from the server.
    pub cache: PathBuf,
    /// The desktop's autostart entries.
    pub autostart: PathBuf,
}

impl Places {
    /// The places for this person, from the environment.
    pub fn from_environment() -> Self {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        let base = |variable: &str, fallback: &str| {
            std::env::var_os(variable)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .unwrap_or_else(|| home.join(fallback))
        };
        let config = base("XDG_CONFIG_HOME", ".config");
        Self {
            state: base("XDG_STATE_HOME", ".local/state").join("transom"),
            cache: base("XDG_CACHE_HOME", ".cache").join("transom"),
            autostart: config.join("autostart"),
            config: config.join("transom"),
        }
    }

    /// The places under a new, empty directory of the system's temporary
    /// directory, for a test.
    #[cfg(test)]
    pub fn fresh() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static MADE: AtomicU32 = AtomicU32::new(0);
        let (made, process) = (MADE.fetch_add(1, Ordering::Relaxed), std::process::id());
        let root = std::env::temp_dir().join(format!("transom-desktop-{process}-{made}"));
        let _ = std::fs::remove_dir_all(&root);
        Self::under(&root)
    }

    /// The places under one directory, for a test.
    #[cfg(test)]
    pub fn under(root: &Path) -> Self {
        Self {
            state: root.join("state"),
            config: root.join("config"),
            cache: root.join("cache"),
            autostart: root.join("autostart"),
        }
    }

    /// This computer's device id, made the first time it is asked for
    /// and kept across sign-outs, so signing in again registers the same
    /// device instead of adding another.
    pub fn device(&self) -> String {
        let path = self.state.join("device");
        if let Ok(device) = std::fs::read_to_string(&path)
            && !device.trim().is_empty()
        {
            return device.trim().to_owned();
        }
        let mut bytes = [0u8; 16];
        aws_lc_rs::rand::fill(&mut bytes).expect("the system gives random bytes");
        let mut device = String::new();
        for byte in bytes {
            let _ = write!(device, "{byte:02x}");
        }
        let _ = std::fs::create_dir_all(&self.state);
        let _ = std::fs::write(&path, &device);
        device
    }
}

/// What the person has chosen.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Preferences {
    /// Whether this computer shows notifications.
    #[serde(default = "yes")]
    pub notifications: bool,
    /// The server last signed in to, which the sign-in screen offers
    /// again.
    #[serde(default)]
    pub server: Option<String>,
    /// When notifying began for the signed-in session.
    #[serde(default)]
    pub since: Option<jiff::Timestamp>,
}

fn yes() -> bool {
    true
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            notifications: true,
            server: None,
            since: None,
        }
    }
}

impl Preferences {
    pub fn load(places: &Places) -> Self {
        std::fs::read(places.config.join("settings.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, places: &Places) {
        let _ = std::fs::create_dir_all(&places.config);
        let bytes = serde_json::to_vec_pretty(self).expect("preferences are plain data");
        if let Err(error) = std::fs::write(places.config.join("settings.json"), bytes) {
            tracing::warn!(%error, "the preferences could not be saved");
        }
    }
}

/// The autostart entry: the desktop starts the app at login with no
/// window, so only the tray icon and the notifications appear.
fn autostart_entry(program: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=transom\n\
         Comment=Questions and notifications from your programs\n\
         Exec={} --background\n\
         Icon={APP_ID}\n\
         X-GNOME-Autostart-enabled=true\n",
        program.display()
    )
}

fn autostart_path(places: &Places) -> PathBuf {
    places.autostart.join(format!("{APP_ID}.desktop"))
}

/// Whether the desktop starts the app at login.
pub fn starts_at_login(places: &Places) -> bool {
    autostart_path(places).exists()
}

/// Makes the desktop start `program` at login, or stop starting it.
pub fn start_at_login(places: &Places, program: &Path, start: bool) -> std::io::Result<()> {
    let path = autostart_path(places);
    if start {
        std::fs::create_dir_all(&places.autostart)?;
        std::fs::write(path, autostart_entry(program))
    } else if path.exists() {
        std::fs::remove_file(path)
    } else {
        Ok(())
    }
}

/// The app's icons in the source tree, which a build that is not
/// installed draws from. An installed build finds them in the system's
/// icon theme.
const SOURCE_ICONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/data/icons");

/// The directory of icons to add to the icon theme's search, or `None`
/// when `program`, the app's own file, is where a package installed it.
pub fn loose_icons(program: &Path) -> Option<&'static str> {
    let installed = program.starts_with("/usr");
    (!installed && Path::new(SOURCE_ICONS).is_dir()).then_some(SOURCE_ICONS)
}

/// When the computer last heard from its server, as `snapshot` has it at
/// `now`, so the person can trust that messages arrive.
pub fn last_heard(snapshot: &Snapshot, now: Timestamp, zone: &TimeZone) -> String {
    if snapshot.connected {
        return "Connected to the server now.".to_owned();
    }
    match snapshot.heard {
        Some(heard) => format!(
            "Not connected. Last heard from the server {}.",
            describe::ago(heard, now, zone)
        ),
        None => "Not connected. Nothing heard from the server yet.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places(name: &str) -> Places {
        let root =
            std::env::temp_dir().join(format!("transom-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Places::under(&root)
    }

    #[test]
    fn the_device_id_is_made_once() {
        let places = places("device");
        let device = places.device();
        assert_eq!(device.len(), 32);
        assert_eq!(places.device(), device);
    }

    #[test]
    fn preferences_default_to_notifying_and_are_kept() {
        let places = places("preferences");
        let mut preferences = Preferences::load(&places);
        assert_eq!(preferences, Preferences::default());
        assert!(preferences.notifications);
        preferences.notifications = false;
        preferences.server = Some("https://transom.example.com/".to_owned());
        preferences.save(&places);
        assert_eq!(Preferences::load(&places), preferences);
    }

    #[test]
    fn preferences_a_file_leaves_out_take_their_defaults() {
        let places = places("partial");
        std::fs::create_dir_all(&places.config).unwrap();
        std::fs::write(places.config.join("settings.json"), "{}").unwrap();
        assert_eq!(Preferences::load(&places), Preferences::default());
    }

    #[test]
    fn preferences_that_cannot_be_written_are_not_kept() {
        let places = places("unwritable");
        std::fs::create_dir_all(places.config.parent().unwrap()).unwrap();
        std::fs::write(&places.config, "not a directory").unwrap();
        let quiet = Preferences {
            notifications: false,
            ..Preferences::default()
        };
        quiet.save(&places);
        assert_eq!(Preferences::load(&places), Preferences::default());
    }

    #[test]
    fn a_build_that_is_not_installed_draws_its_icons_from_the_source_tree() {
        let built = std::env::current_exe().unwrap();
        let icons = Path::new(loose_icons(&built).unwrap());
        assert!(
            icons
                .join("hicolor/symbolic/apps/sh.transom-symbolic.svg")
                .is_file()
        );
        assert_eq!(loose_icons(Path::new("/usr/bin/transom-desktop")), None);
    }

    /// A snapshot that is `connected`, or not, and last `heard` then.
    fn heard(connected: bool, heard: Option<&str>) -> Snapshot {
        Snapshot {
            connected,
            heard: heard.map(|heard| heard.parse().unwrap()),
            ..Snapshot::default()
        }
    }

    #[test]
    fn the_connection_is_said_as_now_or_as_when_the_server_was_last_heard() {
        let now: Timestamp = "2026-11-19T12:00:00Z".parse().unwrap();
        let earlier = Some("2026-11-19T11:56:00Z");
        let cases = [
            (heard(true, earlier), "Connected to the server now."),
            (
                heard(false, earlier),
                "Not connected. Last heard from the server 4m.",
            ),
            (
                heard(false, None),
                "Not connected. Nothing heard from the server yet.",
            ),
        ];
        for (snapshot, said) in cases {
            assert_eq!(last_heard(&snapshot, now, &TimeZone::UTC), said);
        }
    }

    #[test]
    fn starting_at_login_writes_and_removes_the_entry() {
        let places = places("autostart");
        assert!(!starts_at_login(&places));
        start_at_login(&places, Path::new("/usr/bin/transom"), true).unwrap();
        assert!(starts_at_login(&places));
        let entry = std::fs::read_to_string(autostart_path(&places)).unwrap();
        assert!(entry.contains("Exec=/usr/bin/transom --background\n"));
        start_at_login(&places, Path::new("/usr/bin/transom"), false).unwrap();
        start_at_login(&places, Path::new("/usr/bin/transom"), false).unwrap();
        assert!(!starts_at_login(&places));
    }

    #[test]
    fn the_places_follow_the_environment_or_one_root() {
        let places = Places::under(Path::new("/tmp/root"));
        assert_eq!(places.state, Path::new("/tmp/root/state"));
        assert!(Places::from_environment().state.ends_with("transom"));
    }
}
