//! Waiting for an exchange to change: in a request held open by
//! `Prefer: wait` (RFC 7240), or in a stream of Server-Sent Events.

use std::convert::Infallible;
use std::time::Duration;

use axum::http::HeaderMap;
use axum::http::header::ACCEPT;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::{Stream, StreamExt};

use crate::app::Failure;
use crate::store::Store;

/// The longest a request may wait. A wait asks the server to hold a
/// connection open, and some proxies between client and server drop one
/// held much longer than this.
const LONGEST_WAIT: Duration = Duration::from_hours(1);

/// How long the request asks to wait: `Prefer: wait=600`, or `?wait=10m`.
pub(crate) fn wait(headers: &HeaderMap, query: Option<&str>) -> Result<Option<Duration>, Failure> {
    let preferred = headers
        .get_all("prefer")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .find_map(|preference| preference.trim().strip_prefix("wait="))
        .map(str::to_owned);
    let queried = query.and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(name, _)| name == "wait")
            .map(|(_, value)| value.into_owned())
    });
    preferred
        .or(queried)
        .map(|value| {
            super::fields::ttl(&value)
                .map(|seconds| Duration::from_secs(seconds).min(LONGEST_WAIT))
                .map_err(|_| {
                    Failure::new(
                        axum::http::StatusCode::BAD_REQUEST,
                        format!("wait must be seconds or a duration, not {value:?}"),
                    )
                })
        })
        .transpose()
}

/// Whether the request asks for a stream of events.
pub(crate) fn wants_events(headers: &HeaderMap) -> bool {
    headers
        .get(ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| accept.contains("text/event-stream"))
}

/// Waits up to `wait` for `check` to find what it is looking for, checking
/// again each time a message arrives on `channels`. It subscribes before it
/// first checks, so a change that lands between the two is not missed.
pub(crate) async fn until<T, F>(
    store: &Store,
    channels: &[String],
    wait: Duration,
    mut check: impl FnMut() -> F,
) -> Result<Option<T>, Failure>
where
    F: Future<Output = Result<Option<T>, Failure>>,
{
    let mut subscription = store.subscribe(channels).await?;
    let mut messages = subscription.on_message();
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        if let Some(found) = check().await? {
            return Ok(Some(found));
        }
        if tokio::time::timeout_at(deadline, messages.next())
            .await
            .is_err()
        {
            return Ok(None);
        }
    }
}

/// What a stream sends after each check: events to send, and whether the
/// stream is done.
pub(crate) struct Step {
    pub(crate) events: Vec<Event>,
    pub(crate) done: bool,
}

/// A stream of events: it checks once at the start and again each time a
/// message arrives on `channels`, passing the message's payload, sends
/// the events each check returns, and ends when a check says it is done.
pub(crate) async fn stream<C, F>(
    store: &Store,
    channels: &[String],
    check: C,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + use<C, F>>, Failure>
where
    C: FnMut(Option<String>) -> F + Send + 'static,
    F: Future<Output = Result<Step, Failure>> + Send,
{
    let subscription = store.subscribe(channels).await?;
    let state = (subscription.into_on_message(), check, true, false);
    let events =
        futures_util::stream::unfold(state, |(mut messages, mut check, first, done)| async move {
            if done {
                return None;
            }
            let payload = if first {
                None
            } else {
                Some(
                    messages
                        .next()
                        .await?
                        .get_payload::<String>()
                        .unwrap_or_default(),
                )
            };
            let step = check(payload).await.unwrap_or_else(|failure| Step {
                events: vec![Event::default().event("error").data(failure.to_string())],
                done: true,
            });
            let events = futures_util::stream::iter(step.events.into_iter().map(Ok));
            Some((events, (messages, check, false, step.done)))
        })
        .flatten();
    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}
