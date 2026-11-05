//! A Redis for transom's tests: the one at `TRANSOM_TEST_REDIS_URL`, or
//! one in a Docker container that lives exactly as long as the process
//! that started it.
//!
//! `docker run --interactive` connects the container's standard input to
//! a pipe that only the starting process writes, and the container's first
//! process exits when that input ends. The kernel closes the pipe when the
//! process exits for any reason, a panic, Ctrl-C, or SIGKILL among them,
//! and `--rm` then removes the container. Nothing has to run at exit, so
//! no exit leaves a container behind. Docker publishes the container's
//! port on a free host port of its choosing, so any number of test
//! processes, in any number of checkouts, each run a Redis of their own.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};

/// The image of every test Redis, pinned by digest so each run tests
/// against the same server.
pub const IMAGE: &str =
    "redis:8.8.4-alpine@sha256:ab669f1420eaa05360a417b24f3bd210c9272e2c79a4bb35d3e50b4bfd50dbee";

/// The variable that names a Redis to use in place of a container.
pub const URL_VARIABLE: &str = "TRANSOM_TEST_REDIS_URL";

/// How long a container has to answer, which includes pulling the image
/// on a machine that has never run it.
const STARTUP: Duration = Duration::from_mins(2);

/// The container's first process. The image's entrypoint turns off
/// protected mode only when it starts `redis-server` itself, and a
/// connection through a published port arrives from outside the
/// container, which protected mode refuses. Tests need no persistence.
const SCRIPT: &str = "redis-server --protected-mode no --save '' --appendonly no & read -r _";

/// The URL of the Redis this process's tests share, each under a key
/// prefix of its own: `TRANSOM_TEST_REDIS_URL` when it is set, and
/// otherwise a container this process starts on the first call.
#[must_use]
pub fn url() -> String {
    std::env::var(URL_VARIABLE).unwrap_or_else(|_| shared().url.clone())
}

fn shared() -> &'static Redis {
    static SHARED: OnceLock<Redis> = OnceLock::new();
    SHARED.get_or_init(Redis::start)
}

/// A Redis in a container of its own, removed when this is dropped or
/// when the process exits, whichever comes first.
pub struct Redis {
    url: String,
    container: String,
    docker: Child,
}

impl Redis {
    /// Starts a container and returns once its Redis answers `PING`.
    ///
    /// # Panics
    ///
    /// When `docker` cannot run the container, or its Redis does not
    /// answer within two minutes.
    #[must_use]
    pub fn start() -> Self {
        // The process ID keeps names apart among the processes running
        // now, and the time keeps them apart from a container whose
        // process had the same ID and is still being removed.
        let since = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
        let container = format!(
            "transom-test-redis-{}-{:x}",
            std::process::id(),
            since.unwrap().as_nanos()
        );
        let docker = Command::new("docker")
            .args(["run", "--interactive", "--rm", "--name", &container])
            .args(["--publish", "127.0.0.1::6379", IMAGE, "sh", "-c", SCRIPT])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .expect("the tests start a Redis with docker when TRANSOM_TEST_REDIS_URL is unset");
        // Built before Redis answers, so a panic while waiting drops it
        // and removes the container.
        let mut redis = Self {
            url: String::new(),
            container,
            docker,
        };
        let address = redis.await_answer(Instant::now() + STARTUP);
        redis.url = format!("redis://{address}");
        redis
    }

    /// The Redis URL, such as `redis://127.0.0.1:32768`.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The container's name, for `docker` commands.
    #[must_use]
    pub fn container(&self) -> &str {
        &self.container
    }

    /// Waits for the published port to exist and for Redis to answer on it.
    fn await_answer(&mut self, deadline: Instant) -> SocketAddr {
        loop {
            // docker's own error, such as a failed pull, goes to standard
            // error, which this process shares.
            let running = self.docker.try_wait().unwrap().is_none();
            assert!(running, "docker run exited before Redis answered");
            assert!(
                Instant::now() < deadline,
                "Redis did not answer in {STARTUP:?}"
            );
            if let Some(address) = self.published().filter(|address| answers(*address)) {
                return address;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The host address of the container's port 6379, once Docker has
    /// published it.
    fn published(&self) -> Option<SocketAddr> {
        let output = Command::new("docker")
            .args(["port", &self.container, "6379/tcp"])
            .stderr(Stdio::null())
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().parse().ok()
    }
}

impl Drop for Redis {
    /// Ends the container's input and waits for `docker run` to exit,
    /// which it does once `--rm` has removed the container.
    fn drop(&mut self) {
        drop(self.docker.stdin.take());
        self.docker.wait().unwrap();
    }
}

/// Whether a Redis at `address` answers `PING`. Docker accepts
/// connections on a published port before Redis listens, and closes
/// them, so a connection alone shows nothing.
fn answers(address: SocketAddr) -> bool {
    let timeout = Some(Duration::from_secs(1));
    let mut reply = [0; 7];
    TcpStream::connect(address)
        .and_then(|mut stream| {
            stream.set_read_timeout(timeout)?;
            stream.write_all(b"PING\r\n")?;
            stream.read_exact(&mut reply)
        })
        .is_ok_and(|()| &reply == b"+PONG\r\n")
}
