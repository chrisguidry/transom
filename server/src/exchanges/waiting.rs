//! Waiting for an exchange to change: in a request held open by
//! `Prefer: wait` (RFC 7240), or in a stream of Server-Sent Events.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::http::HeaderMap;
use axum::http::header::ACCEPT;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;

use crate::app::{App, Failure};
use crate::signin::sessions::{self, Checked, Session};
use crate::store::{Heard, Subscription};

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
/// again each time a message arrives on `channels`, and each time the
/// subscription resumes after a dropped connection. It subscribes before
/// it first checks, so a change that lands between the two is not missed.
/// It stops waiting when the server stops, so its client asks again of
/// another server.
pub(crate) async fn until<T, F>(
    app: &App,
    channels: &[String],
    wait: Duration,
    mut check: impl FnMut() -> F,
) -> Result<Option<T>, Failure>
where
    F: Future<Output = Result<Option<T>, Failure>>,
{
    let mut subscription = app.store.subscribe(channels).await?;
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        if let Some(found) = check().await? {
            return Ok(Some(found));
        }
        tokio::select! {
            _ = subscription.next() => {}
            () = tokio::time::sleep_until(deadline) => return Ok(None),
            () = app.stopping.cancelled() => return Ok(None),
        }
    }
}

/// What a stream sends after each check: events to send, and whether the
/// stream is done.
pub(crate) struct Step {
    pub(crate) events: Vec<Event>,
    pub(crate) done: bool,
}

/// The session an event stream was opened with, which the stream checks
/// again while it runs, so it closes when the session ends.
struct Watch {
    app: Arc<App>,
    digest: String,
    /// The full name of the channel that announces the session's end.
    channel: String,
    /// When the session is next checked, for a session that ends without
    /// a word, as one that expires unused does, and so its issuer is asked
    /// as often as a session that makes requests.
    tick: tokio::time::Interval,
}

impl Watch {
    fn new(app: &Arc<App>, digest: String) -> Self {
        let every = app.session_checks;
        Self {
            app: app.clone(),
            channel: app.store.key(&sessions::channel(&digest)),
            digest,
            tick: tokio::time::interval_at(tokio::time::Instant::now() + every, every),
        }
    }

    /// Whether the session may still be current. A check that fails, as
    /// when Redis fails over, keeps the stream, and the next check asks
    /// again: closing it would close every session's stream at once.
    async fn current(&self) -> bool {
        !matches!(
            sessions::check(&self.app, &self.digest).await,
            Ok(Checked::Ended(_))
        )
    }
}

/// The next thing a stream hears for its check, or `None` when the stream
/// should end: the server is stopping, or `watch`'s session has ended. A
/// message on the session's own channel, or a subscription that resumed
/// and may have missed one, has the session checked again.
async fn next(
    subscription: &mut Subscription,
    watch: &mut Option<Watch>,
    stopping: &CancellationToken,
) -> Option<Heard> {
    loop {
        let ticked = async {
            match watch {
                Some(watch) => drop(watch.tick.tick().await),
                None => std::future::pending().await,
            }
        };
        let heard = tokio::select! {
            heard = subscription.next() => Some(heard),
            () = ticked => None,
            () = stopping.cancelled() => return None,
        };
        // Without a session to watch nothing ticks, so `heard` is always
        // something the subscription heard.
        let Some(watch) = watch.as_ref() else {
            return heard;
        };
        let (recheck, pass_on) = match &heard {
            None => (true, false),
            Some(Heard::Message { channel, .. }) if *channel == watch.channel => (true, false),
            Some(Heard::Resumed) => (true, true),
            Some(Heard::Message { .. }) => (false, true),
        };
        if recheck && !watch.current().await {
            return None;
        }
        if pass_on {
            return heard;
        }
    }
}

/// A stream of events: it checks once at the start, with `None`, and again
/// with what it heard each time a message arrives on `channels` or the
/// subscription resumes after a dropped connection. It sends the events
/// each check returns, and ends when a check says it is done, when the
/// server stops, so its client reconnects to another server, or when
/// `session`, the digest of the session the stream was opened with, ends.
pub(crate) async fn stream<C, F>(
    app: &Arc<App>,
    channels: &[String],
    session: Option<&Session>,
    check: C,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + use<C, F>>, Failure>
where
    C: FnMut(Option<Heard>) -> F + Send + 'static,
    F: Future<Output = Result<Step, Failure>> + Send,
{
    let watch = session.map(|session| Watch::new(app, session.digest.clone()));
    let mut channels = channels.to_vec();
    channels.extend(session.map(|session| sessions::channel(&session.digest)));
    let subscription = app.store.subscribe(&channels).await?;
    let stopping = app.stopping.clone();
    let state = (subscription, watch, check, true, false);
    let events = futures_util::stream::unfold(
        state,
        move |(mut subscription, mut watch, mut check, first, done)| {
            let stopping = stopping.clone();
            async move {
                if done {
                    return None;
                }
                let heard = if first {
                    None
                } else {
                    Some(next(&mut subscription, &mut watch, &stopping).await?)
                };
                let step = check(heard).await.unwrap_or_else(|failure| Step {
                    events: vec![Event::default().event("error").data(failure.to_string())],
                    done: true,
                });
                let events = futures_util::stream::iter(step.events.into_iter().map(Ok));
                Some((events, (subscription, watch, check, false, step.done)))
            }
        },
    )
    .flatten();
    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}
