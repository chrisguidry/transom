//! The live feed at `/@{name}/exchanges`: a stream of Server-Sent Events
//! that sends each exchange in the list again when it changes.

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::Method;
use serde::Deserialize;

use super::client::{Client, Failure};
use crate::exchange::Exchange;

/// The server sends a comment every 15 seconds on a feed with nothing to
/// say, so a read that waits longer than this has lost its connection,
/// though no error said so.
const SILENCE: Duration = Duration::from_secs(45);

/// What the feed says after it opens.
#[derive(Debug, PartialEq)]
pub enum Event {
    /// An exchange in the list changed, or its read state did; this is
    /// all of it, with `read`.
    Changed(Box<Exchange>),
    /// The server deleted the exchange, or the group that received it.
    Gone(String),
}

/// One open connection to the feed. It never reconnects: the caller does
/// that.
pub struct Feed {
    chunks: futures_util::stream::BoxStream<'static, reqwest::Result<Vec<u8>>>,
    lines: Lines,
}

impl Feed {
    /// Opens `name`'s feed. The server subscribes before it answers, so a
    /// change made after this returns is never missed, and whatever
    /// changed before has to be read from the list.
    pub async fn open(client: &Client, name: &str) -> Result<Self, Failure> {
        let request = client
            .signed(Method::GET, client.url(&[name, "exchanges"]))
            .header("Accept", "text/event-stream");
        let response = tokio::time::timeout(SILENCE, request.send())
            .await
            .map_err(|_| Failure::Unreachable("the feed did not open".to_owned()))??;
        if !response.status().is_success() {
            return Err(client.report(Failure::of(response).await));
        }
        let chunks = response
            .bytes_stream()
            .map(|chunk| chunk.map(|bytes| bytes.to_vec()))
            .boxed();
        Ok(Self {
            chunks,
            lines: Lines::default(),
        })
    }

    /// The next event, or `None` when the server closed the feed. An
    /// event of a type this app does not know is skipped, and so is the
    /// `error` event the server ends a feed with.
    pub async fn next(&mut self) -> Result<Option<Event>, Failure> {
        loop {
            if let Some((kind, data)) = self.lines.event() {
                if let Some(event) = decode(&kind, &data) {
                    return Ok(Some(event));
                }
                continue;
            }
            let chunk = tokio::time::timeout(SILENCE, self.chunks.next())
                .await
                .map_err(|_| Failure::Unreachable("the feed went silent".to_owned()))?;
            match chunk {
                Some(chunk) => self.lines.push(&chunk?),
                None => return Ok(None),
            }
        }
    }
}

fn decode(kind: &str, data: &str) -> Option<Event> {
    #[derive(Deserialize)]
    struct GoneId {
        id: String,
    }
    match kind {
        "exchange" => serde_json::from_str(data).ok().map(Event::Changed),
        "gone" => serde_json::from_str::<GoneId>(data)
            .ok()
            .map(|gone| Event::Gone(gone.id)),
        _ => None,
    }
}

/// The bytes of an event stream, split into events: each is its `event:`
/// field and its `data:` lines joined, ended by a blank line.
#[derive(Default)]
struct Lines {
    buffer: Vec<u8>,
    kind: String,
    data: Vec<String>,
}

impl Lines {
    fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next whole event in the buffer, if one has arrived.
    fn event(&mut self) -> Option<(String, String)> {
        while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                let kind = std::mem::take(&mut self.kind);
                let data = std::mem::take(&mut self.data).join("\n");
                if !kind.is_empty() || !data.is_empty() {
                    return Some((kind, data));
                }
            } else if let Some(kind) = line.strip_prefix("event:") {
                kind.trim_start().clone_into(&mut self.kind);
            } else if let Some(data) = line.strip_prefix("data:") {
                self.data
                    .push(data.strip_prefix(' ').unwrap_or(data).to_owned());
            }
        }
        None
    }
}

/// How long to wait before reconnecting after `failures` connections in a
/// row have failed: a second, doubling each time, up to a minute.
pub fn backoff(failures: u32) -> Duration {
    Duration::from_secs(1u64 << failures.min(6)).min(Duration::from_secs(60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_split_at_blank_lines_across_chunks() {
        let mut lines = Lines::default();
        lines.push(b": keep-alive\n\nevent: gone\ndata: {\"id\":");
        assert_eq!(lines.event(), None);
        lines.push(b" \"01JB\"}\n\nevent: exch");
        let (kind, data) = lines.event().unwrap();
        assert_eq!(decode(&kind, &data), Some(Event::Gone("01JB".to_owned())));
        assert_eq!(lines.event(), None);
    }

    #[test]
    fn an_unknown_event_is_skipped() {
        assert_eq!(decode("error", "the feed's check failed"), None);
        assert_eq!(decode("exchange", "not json"), None);
    }

    /// A feed that reads `chunks` in place of a server's answer.
    fn reading(chunks: &[&str]) -> Feed {
        let chunks = chunks.iter().map(|chunk| Ok(chunk.as_bytes().to_vec()));
        Feed {
            chunks: futures_util::stream::iter(chunks.collect::<Vec<_>>()).boxed(),
            lines: Lines::default(),
        }
    }

    #[tokio::test]
    async fn the_event_after_one_the_app_does_not_know_is_the_next() {
        let error = "event: error\ndata: the feed's check failed\n\n";
        let mut feed = reading(&[error, "event: gone\ndata: {\"id\": \"01JB\"}\n\n"]);
        let next = feed.next().await.unwrap();
        assert_eq!(next, Some(Event::Gone("01JB".to_owned())));
        assert_eq!(feed.next().await.unwrap(), None);
    }

    #[test]
    fn the_backoff_doubles_up_to_a_minute() {
        let waits: Vec<u64> = (0..8).map(|failures| backoff(failures).as_secs()).collect();
        assert_eq!(waits, [1, 2, 4, 8, 16, 32, 60, 60]);
    }
}
