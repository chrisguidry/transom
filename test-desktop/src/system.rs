//! The two services of the system bus that the desktop app listens to:
//! logind, which says when the computer sleeps and wakes, and
//! `NetworkManager`, which says when the network's state changes. Each
//! sends its one signal when a test asks.

use zbus::object_server::SignalEmitter;

const LOGIN_PATH: &str = "/org/freedesktop/login1";
const NETWORK_PATH: &str = "/org/freedesktop/NetworkManager";

/// `NetworkManager`'s state for a connection that reaches the internet,
/// `NM_STATE_CONNECTED_GLOBAL`.
pub const CONNECTED_GLOBAL: u32 = 70;

/// `NetworkManager`'s state for a connection that reaches only its own
/// site, `NM_STATE_CONNECTED_SITE`.
pub const CONNECTED_SITE: u32 = 60;

/// `NetworkManager`'s state with no connection, `NM_STATE_DISCONNECTED`.
pub const DISCONNECTED: u32 = 20;

/// logind and `NetworkManager` on one bus connection.
///
/// ```no_run
/// # async fn example() -> zbus::Result<()> {
/// let bus = test_desktop::TestBus::start();
/// let system = test_desktop::TestSystem::serve(bus.connect().await?).await?;
/// // … the app, on a connection of its own, listens …
/// system.wake().await?;
/// # Ok(())
/// # }
/// ```
pub struct TestSystem {
    connection: zbus::Connection,
}

impl TestSystem {
    /// Serves both services on `connection` and takes their well-known
    /// names.
    ///
    /// # Errors
    ///
    /// Fails when the bus refuses a service's path or name.
    pub async fn serve(connection: zbus::Connection) -> zbus::Result<Self> {
        let server = connection.object_server();
        server.at(LOGIN_PATH, Login).await?;
        server.at(NETWORK_PATH, Network).await?;
        for name in ["org.freedesktop.login1", "org.freedesktop.NetworkManager"] {
            connection.request_name(name).await?;
        }
        Ok(Self { connection })
    }

    /// Says the computer is about to sleep.
    ///
    /// # Errors
    ///
    /// Fails when the bus does not take the signal.
    pub async fn sleep(&self) -> zbus::Result<()> {
        let emitter = SignalEmitter::new(&self.connection, LOGIN_PATH)?;
        Login::prepare_for_sleep(&emitter, true).await
    }

    /// Says the computer woke.
    ///
    /// # Errors
    ///
    /// Fails when the bus does not take the signal.
    pub async fn wake(&self) -> zbus::Result<()> {
        let emitter = SignalEmitter::new(&self.connection, LOGIN_PATH)?;
        Login::prepare_for_sleep(&emitter, false).await
    }

    /// Says the network's state is now `state`, one of
    /// `NetworkManager`'s `NM_STATE` numbers.
    ///
    /// # Errors
    ///
    /// Fails when the bus does not take the signal.
    pub async fn network(&self, state: u32) -> zbus::Result<()> {
        let emitter = SignalEmitter::new(&self.connection, NETWORK_PATH)?;
        Network::state_changed(&emitter, state).await
    }
}

struct Login;

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl Login {
    #[zbus(signal)]
    async fn prepare_for_sleep(emitter: &SignalEmitter<'_>, start: bool) -> zbus::Result<()>;
}

struct Network;

#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Network {
    #[zbus(signal)]
    async fn state_changed(emitter: &SignalEmitter<'_>, state: u32) -> zbus::Result<()>;
}
