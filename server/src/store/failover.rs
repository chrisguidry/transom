//! A stand-in for the address of a Redis primary while the primary moves:
//! a TCP proxy to the test Redis that can drop every connection it
//! carries, refuse new ones, or stop answering while holding them open.

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Carries connections to Redis.
    Open,
    /// Closes each new connection as it arrives.
    Refusing,
    /// Holds connections open and carries nothing over them, as the
    /// address of a primary that disappeared does.
    Silent,
}

pub(crate) struct Failover {
    /// A Redis URL that reaches the test Redis through the proxy.
    pub(crate) url: String,
    mode: watch::Sender<Mode>,
    /// Counts the cuts, so each connection ends at the next one.
    cuts: watch::Sender<u64>,
}

impl Failover {
    pub(crate) async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut url = url::Url::parse(&super::testing::redis_url()).unwrap();
        let redis = format!(
            "{}:{}",
            url.host_str().unwrap(),
            url.port_or_known_default().unwrap()
        );
        url.set_port(Some(listener.local_addr().unwrap().port()))
            .unwrap();
        let mode = watch::Sender::new(Mode::Open);
        let cuts = watch::Sender::new(0);
        let (modes, cutting) = (mode.subscribe(), cuts.subscribe());
        let redis = Arc::new(redis);
        tokio::spawn(async move {
            loop {
                let (client, _) = listener.accept().await.unwrap();
                if *modes.borrow() == Mode::Refusing {
                    continue;
                }
                let (redis, modes, cutting) = (redis.clone(), modes.clone(), cutting.clone());
                tokio::spawn(carry(client, redis, modes, cutting));
            }
        });
        Self {
            url: url.to_string(),
            mode,
            cuts,
        }
    }

    /// Drops every connection the proxy carries now.
    pub(crate) fn cut(&self) {
        self.cuts.send_modify(|cuts| *cuts += 1);
    }

    /// Drops every connection and refuses new ones until [`Self::recover`].
    pub(crate) fn refuse(&self) {
        self.mode.send_replace(Mode::Refusing);
        self.cut();
    }

    /// Stops carrying anything, holding connections open, until
    /// [`Self::recover`].
    pub(crate) fn go_silent(&self) {
        self.mode.send_replace(Mode::Silent);
    }

    /// Carries connections again, as the new primary does.
    pub(crate) fn recover(&self) {
        self.mode.send_replace(Mode::Open);
    }
}

/// Carries one connection until it closes or is cut, forwarding nothing
/// while the proxy is silent.
async fn carry(
    mut client: TcpStream,
    redis: Arc<String>,
    mut modes: watch::Receiver<Mode>,
    mut cutting: watch::Receiver<u64>,
) {
    let mut server = TcpStream::connect(redis.as_str()).await.unwrap();
    cutting.mark_unchanged();
    let (mut from_client, mut to_client) = client.split();
    let (mut from_server, mut to_server) = server.split();
    let (mut up, mut down) = ([0; 4096], [0; 4096]);
    loop {
        let carrying = async {
            modes.wait_for(|mode| *mode != Mode::Silent).await.unwrap();
            tokio::select! {
                carried = forward(&mut from_client, &mut to_server, &mut up) => carried,
                carried = forward(&mut from_server, &mut to_client, &mut down) => carried,
                // A change to silence stops carrying at once.
                _ = modes.changed() => true,
            }
        };
        let carried = tokio::select! {
            carried = carrying => carried,
            _ = cutting.changed() => false,
        };
        if !carried {
            return;
        }
    }
}

/// Forwards what `from` sends next to `to`, and says whether the
/// connection is still open.
async fn forward(
    from: &mut (impl AsyncRead + Unpin),
    to: &mut (impl AsyncWrite + Unpin),
    buffer: &mut [u8],
) -> bool {
    match from.read(buffer).await {
        Ok(0) | Err(_) => false,
        Ok(read) => to.write_all(&buffer[..read]).await.is_ok(),
    }
}
