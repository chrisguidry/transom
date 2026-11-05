//! A subscription to channels that lasts longer than any one connection.

use std::time::Duration;

use futures_util::StreamExt;
use redis::aio::{PubSubSink, PubSubStream};

use super::REDIS_TIMEOUT;

/// How often a quiet subscription checks that Redis still answers on its
/// connection. A primary that disappears in a failover can leave the
/// connection open with nothing at the other end, which would otherwise
/// leave the subscription waiting for messages that never come.
const PING_EVERY: Duration = Duration::from_secs(15);

/// How long a subscription waits before it tries again to subscribe.
const RESUBSCRIBE_AFTER: Duration = Duration::from_millis(500);

/// What a subscription heard.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Heard {
    /// A message, with its payload.
    Message(String),
    /// The connection dropped and the subscription is made again. What was
    /// published while it was down is lost, so whoever listens checks
    /// again for what it is waiting for.
    Resumed,
}

pub(crate) struct Subscription {
    client: redis::Client,
    /// The channels' full names.
    channels: Vec<String>,
    ping_every: Duration,
    sink: PubSubSink,
    messages: PubSubStream,
}

impl Subscription {
    pub(super) async fn open(
        client: redis::Client,
        channels: Vec<String>,
    ) -> redis::RedisResult<Self> {
        Self::pinging(client, channels, PING_EVERY).await
    }

    /// A subscription that checks its connection every `ping_every`.
    pub(super) async fn pinging(
        client: redis::Client,
        channels: Vec<String>,
        ping_every: Duration,
    ) -> redis::RedisResult<Self> {
        let (sink, messages) = connect(&client, &channels).await?;
        Ok(Self {
            client,
            channels,
            ping_every,
            sink,
            messages,
        })
    }

    /// The next thing the subscription hears. When its connection drops,
    /// or stops answering, it subscribes again, trying until Redis takes
    /// the subscription.
    pub(crate) async fn next(&mut self) -> Heard {
        loop {
            match tokio::time::timeout(self.ping_every, self.messages.next()).await {
                Ok(Some(message)) => {
                    return Heard::Message(message.get_payload().unwrap_or_default());
                }
                Ok(None) => break,
                Err(_) => {
                    let pinged = tokio::time::timeout(REDIS_TIMEOUT, self.sink.ping::<()>()).await;
                    if !matches!(pinged, Ok(Ok(()))) {
                        break;
                    }
                }
            }
        }
        loop {
            match connect(&self.client, &self.channels).await {
                Ok((sink, messages)) => {
                    self.sink = sink;
                    self.messages = messages;
                    return Heard::Resumed;
                }
                Err(error) => {
                    tracing::warn!(%error, "a subscription could not be made again");
                    tokio::time::sleep(RESUBSCRIBE_AFTER).await;
                }
            }
        }
    }
}

/// A new connection, subscribed to `channels`.
async fn connect(
    client: &redis::Client,
    channels: &[String],
) -> redis::RedisResult<(PubSubSink, PubSubStream)> {
    let subscribing = async {
        let mut pubsub = client.get_async_pubsub().await?;
        pubsub.subscribe(channels).await?;
        Ok(pubsub.split())
    };
    tokio::time::timeout(REDIS_TIMEOUT, subscribing)
        .await
        .unwrap_or_else(|_| {
            Err(redis::RedisError::from((
                redis::ErrorKind::Io,
                "Redis did not take the subscription in time",
            )))
        })
}
