//! A message bus for one test: a `dbus-daemon` of its own, which lives
//! exactly as long as the `TestBus` that started it, or the process that
//! holds it, whichever ends first.
//!
//! The daemon runs under a shell that reads its standard input, a pipe
//! that only the starting process writes. The kernel closes the pipe
//! when that process exits for any reason, a panic, Ctrl-C, or SIGKILL
//! among them, and the shell then ends the daemon. Nothing has to run at
//! exit, so no exit leaves a daemon behind. The daemon listens on a
//! socket of its choosing, so any number of tests, in any number of
//! processes, each run a bus of their own.
//!
//! ```no_run
//! # async fn example() -> zbus::Result<()> {
//! let bus = test_desktop::TestBus::start();
//! let desktop = test_desktop::TestDesktop::serve(bus.connect().await?).await?;
//! # Ok(())
//! # }
//! ```

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

/// The daemon's configuration, which starts no service by its name.
const CONFIGURATION: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/bus.conf");

/// The shell's script: the daemon, with the configuration file the
/// script is given, until standard input ends. The shell closes its own
/// copy of standard output, so the output ends when a daemon that could
/// not start exits.
const SCRIPT: &str = r#"dbus-daemon --nofork --print-address --config-file="$1" &
exec >/dev/null
read -r _
kill $!"#;

/// A private message bus, ended when this is dropped or when the process
/// exits, whichever comes first.
pub struct TestBus {
    address: String,
    shell: Child,
}

impl TestBus {
    /// Starts a bus and returns once it listens.
    ///
    /// # Panics
    ///
    /// When `dbus-daemon` cannot run.
    #[must_use]
    pub fn start() -> Self {
        let mut shell = Command::new("sh")
            .args(["-c", SCRIPT, "sh", CONFIGURATION])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the tests start a message bus with sh and dbus-daemon");
        let printed = shell.stdout.take().expect("the shell's output is a pipe");
        // Built before the daemon prints its address, so a panic while
        // reading drops it and ends the shell.
        let mut bus = Self {
            address: String::new(),
            shell,
        };
        // dbus-daemon's own error goes to standard error, which this
        // process shares.
        BufReader::new(printed)
            .read_line(&mut bus.address)
            .expect("the daemon's output is text");
        bus.address.truncate(bus.address.trim_end().len());
        assert!(
            !bus.address.is_empty(),
            "dbus-daemon exited before it listened"
        );
        bus
    }

    /// The bus's address, such as `unix:abstract=/tmp/dbus-OUhJGK2Qhb`.
    #[must_use]
    pub fn address(&self) -> &str {
        &self.address
    }

    /// A new connection to the bus.
    ///
    /// # Errors
    ///
    /// Fails when the bus refuses the connection.
    pub async fn connect(&self) -> zbus::Result<zbus::Connection> {
        zbus::connection::Builder::address(self.address())
            .build()
            .await
    }
}

impl Drop for TestBus {
    /// Ends the shell's input and waits for it to exit, which it does
    /// once it has ended the daemon.
    fn drop(&mut self) {
        drop(self.shell.stdin.take());
        self.shell.wait().expect("the shell was started");
    }
}
