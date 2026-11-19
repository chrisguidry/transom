//! The moments a lost connection is worth trying again at once: the
//! computer woke from sleep, or the network came back. Each arrives as a
//! signal on the system bus, so the app never asks on a timer.

use futures_util::StreamExt;

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Login {
    /// `true` just before the computer sleeps, and `false` once it wakes.
    #[zbus(signal)]
    fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait Network {
    #[zbus(signal)]
    fn state_changed(&self, state: u32) -> zbus::Result<()>;
}

/// `NetworkManager`'s state for a connection that reaches the internet,
/// `NM_STATE_CONNECTED_GLOBAL`, and for one that reaches only its own
/// site, which a server on the home network still answers on.
const CONNECTED: [u32; 2] = [70, 60];

/// Calls `woke` each time the computer wakes or the network connects,
/// for as long as `system`, a connection to the system bus, is open. A
/// computer without logind or `NetworkManager` never calls it, and the
/// feed's own backoff reconnects.
pub async fn watch(system: zbus::Connection, woke: impl Fn() + Send + 'static) {
    let sleeps = async {
        let proxy = LoginProxy::new(&system).await.ok()?;
        proxy.receive_prepare_for_sleep().await.ok()
    }
    .await;
    let networks = async {
        let proxy = NetworkProxy::new(&system).await.ok()?;
        proxy.receive_state_changed().await.ok()
    }
    .await;
    let mut sleeps =
        sleeps.map_or_else(|| futures_util::stream::pending().boxed(), StreamExt::boxed);
    let mut networks =
        networks.map_or_else(|| futures_util::stream::pending().boxed(), StreamExt::boxed);
    loop {
        tokio::select! {
            Some(signal) = sleeps.next() => {
                if signal.args().is_ok_and(|args| !args.start) {
                    woke();
                }
            }
            Some(signal) = networks.next() => {
                if signal.args().is_ok_and(|args| CONNECTED.contains(&args.state)) {
                    woke();
                }
            }
            else => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use test_desktop::system::{CONNECTED_GLOBAL, CONNECTED_SITE, DISCONNECTED};
    use test_desktop::{TestBus, TestSystem};
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
    use tokio::task::JoinHandle;

    use super::*;

    /// logind and `NetworkManager` on a bus of their own, the app
    /// watching them, and each wake it was told of.
    struct Watching {
        system: TestSystem,
        wakes: UnboundedReceiver<()>,
        task: JoinHandle<()>,
        bus: TestBus,
    }

    impl Watching {
        /// How many wakes the app has been told of, once it has heard
        /// every signal sent so far: a wake sent last is always told, so
        /// what was sent before it has been heard by the time it arrives.
        async fn wakes_after_one_more(&mut self) -> usize {
            self.system.wake().await.unwrap();
            let last = tokio::time::timeout(Duration::from_secs(10), self.wakes.recv());
            last.await.expect("the wake is heard").unwrap();
            self.wakes.len()
        }
    }

    async fn watching() -> Watching {
        let bus = TestBus::start();
        let system = TestSystem::serve(bus.connect().await.unwrap());
        let system = system.await.unwrap();
        let (sender, mut wakes) = unbounded_channel();
        let woke = move || {
            let _ = sender.send(());
        };
        let task = tokio::spawn(watch(bus.connect().await.unwrap(), woke));
        // The app has subscribed to the signals once it hears a wake. It
        // never hears one sent before that, so another is sent.
        let heard = async |wakes: &mut UnboundedReceiver<()>| {
            system.wake().await.unwrap();
            wakes.recv().await;
        };
        while tokio::time::timeout(Duration::from_secs(1), heard(&mut wakes))
            .await
            .is_err()
        {}
        Watching {
            system,
            wakes,
            task,
            bus,
        }
    }

    #[tokio::test]
    async fn waking_from_sleep_is_told_and_going_to_sleep_is_not() {
        let mut watching = watching().await;
        watching.system.sleep().await.unwrap();
        assert_eq!(watching.wakes_after_one_more().await, 0);
    }

    #[tokio::test]
    async fn a_network_that_connects_is_told() {
        let mut watching = watching().await;
        watching.system.network(CONNECTED_GLOBAL).await.unwrap();
        watching.system.network(CONNECTED_SITE).await.unwrap();
        assert_eq!(watching.wakes_after_one_more().await, 2);
    }

    #[tokio::test]
    async fn a_network_that_disconnects_is_not_told() {
        let mut watching = watching().await;
        watching.system.network(DISCONNECTED).await.unwrap();
        assert_eq!(watching.wakes_after_one_more().await, 0);
    }

    #[tokio::test]
    async fn the_watch_ends_with_the_bus() {
        let Watching { task, bus, .. } = watching().await;
        drop(bus);
        let ended = tokio::time::timeout(Duration::from_secs(10), task);
        ended.await.expect("the watch ends").unwrap();
    }
}
