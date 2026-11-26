//! What transom counts for Prometheus, served at `/metrics` on a listener
//! of its own (`scrape.rs`), so the port people reach never serves it.
//!
//! Each server counts only what it did itself, so a query sums a counter
//! across servers. The queue gauges are read from Redis when Prometheus
//! scrapes, so every server reports the same values.
//!
//! Every label value comes from an enum here, an issuer's name in the
//! settings, or a route the router declares, so no request, identity, or
//! exchange can add a series.

pub(crate) mod scrape;
#[cfg(test)]
mod tests;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::{Histogram, exponential_buckets};
use prometheus_client::registry::Registry;

use crate::exchanges::records::{Exchange, State};
use crate::store::Store;

/// The release this binary was built for, as the image build passes it,
/// or `dev` for any other build.
fn version() -> &'static str {
    option_env!("TRANSOM_VERSION")
        .filter(|version| !version.is_empty())
        .unwrap_or("dev")
}

/// How an exchange was sent.
#[derive(Clone, Copy)]
pub(crate) enum Via {
    Http,
    Mcp,
}

impl Via {
    fn as_str(self) -> &'static str {
        match self {
            Via::Http => "http",
            Via::Mcp => "mcp",
        }
    }
}

/// What an attempt delivered, which names the queue it came from.
#[derive(Clone, Copy)]
pub(crate) enum Channel {
    Webhook,
    Push,
    Wake,
}

impl Channel {
    const ALL: [Channel; 3] = [Channel::Webhook, Channel::Push, Channel::Wake];

    fn as_str(self) -> &'static str {
        match self {
            Channel::Webhook => "webhook",
            Channel::Push => "push",
            Channel::Wake => "wake",
        }
    }
}

/// What became of one attempt to deliver something.
#[derive(Clone, Copy)]
pub(crate) enum Outcome {
    /// The receiver took it.
    Delivered,
    /// The receiver answered with a status that trying again does not
    /// change.
    Refused,
    /// No answer came: the connection failed or timed out.
    Unreachable,
    /// The receiver asked to be tried again later.
    Busy,
    /// The receiver said the device no longer exists, and transom forgot
    /// it.
    Forgotten,
}

impl Outcome {
    const ALL: [Outcome; 5] = [
        Outcome::Delivered,
        Outcome::Refused,
        Outcome::Unreachable,
        Outcome::Busy,
        Outcome::Forgotten,
    ];

    fn as_str(self) -> &'static str {
        match self {
            Outcome::Delivered => "delivered",
            Outcome::Refused => "refused",
            Outcome::Unreachable => "unreachable",
            Outcome::Busy => "busy",
            Outcome::Forgotten => "forgotten",
        }
    }
}

/// Why a request's sign-in failed.
#[derive(Clone, Copy)]
pub(crate) enum Refusal {
    /// The request carried no token and no session cookie.
    NoCredentials,
    /// The token is not a JWT whose issuer can be read.
    Malformed,
    /// The token's issuer is not one the settings name.
    UntrustedIssuer,
    /// The issuer's keys or claims rejected the token.
    Invalid,
    /// The issuer's keys could not be fetched.
    IssuerUnavailable,
    /// The session is gone: it was signed out, expired unused, or its
    /// device was forgotten.
    SessionEnded,
    /// The session's issuer refused its refresh token, which ended it.
    IssuerEnded,
    /// A cookie came on a request that transom's own pages did not make.
    CrossSite,
}

impl Refusal {
    /// The reason as a label, and as the `code` of a refused request's
    /// body, so a client and a dashboard name a refusal alike.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Refusal::NoCredentials => "no_credentials",
            Refusal::Malformed => "malformed",
            Refusal::UntrustedIssuer => "untrusted_issuer",
            Refusal::Invalid => "invalid",
            Refusal::IssuerUnavailable => "issuer_unavailable",
            Refusal::SessionEnded => "session_ended",
            Refusal::IssuerEnded => "issuer_ended",
            Refusal::CrossSite => "cross_site",
        }
    }
}

/// What became of one refresh of a session with its issuer.
#[derive(Clone, Copy)]
pub(crate) enum Refresh {
    /// The issuer issued new tokens.
    Refreshed,
    /// The issuer refused the refresh token with `invalid_grant`, which
    /// ended the session.
    Ended,
    /// The issuer refused with another error, or refused a refresh token
    /// the session had already replaced, and the session carries on.
    Refused,
    /// The issuer could not be reached or did not answer in time, and the
    /// session carries on.
    Unavailable,
}

impl Refresh {
    fn as_str(self) -> &'static str {
        match self {
            Refresh::Refreshed => "refreshed",
            Refresh::Ended => "ended",
            Refresh::Refused => "refused",
            Refresh::Unavailable => "unavailable",
        }
    }
}

/// The label of an exchange's kind.
fn kind(question: bool) -> &'static str {
    if question { "question" } else { "notification" }
}

/// A method as a label: one of the methods transom's routes take, or
/// `other`, since a client can send any word as a method.
fn method(method: &Method) -> &'static str {
    match *method {
        Method::GET => "GET",
        Method::HEAD => "HEAD",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::DELETE => "DELETE",
        Method::PATCH => "PATCH",
        Method::OPTIONS => "OPTIONS",
        _ => "other",
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Version {
    version: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Answered {
    method: &'static str,
    route: String,
    status: u16,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Route {
    method: &'static str,
    route: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Sent {
    kind: &'static str,
    issuer: String,
    via: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Ended {
    kind: &'static str,
    state: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Attempted {
    queue: &'static str,
    outcome: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Dropped {
    queue: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Refreshed {
    outcome: &'static str,
    issuer: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Refused {
    reason: &'static str,
    issuer: String,
}

/// The buckets of a request's duration, wide enough that a request that
/// waits for a reply still lands in one below the overflow bucket.
fn durations() -> Histogram {
    Histogram::new(exponential_buckets(0.005, 2.0, 14))
}

pub(crate) struct Metrics {
    registry: Registry,
    answered: Family<Answered, Counter>,
    durations: Family<Route, Histogram, fn() -> Histogram>,
    sent: Family<Sent, Counter>,
    ended: Family<Ended, Counter>,
    attempted: Family<Attempted, Counter>,
    dropped: Family<Dropped, Counter>,
    maintenance_failures: Counter,
    refused: Family<Refused, Counter>,
    refreshes: Family<Refreshed, Counter>,
}

impl Metrics {
    /// The metrics of a server on `store`, which counts its own Redis
    /// errors and subscriptions.
    pub(crate) fn new(store: &Store) -> Self {
        let mut registry = Registry::default();
        let build = Family::<Version, Gauge>::default();
        build.get_or_create(&Version { version: version() }).set(1);
        registry.register("transom_build_info", "The release this server runs", build);
        let metrics = Self {
            registry,
            answered: Family::default(),
            durations: Family::new_with_constructor(durations),
            sent: Family::default(),
            ended: Family::default(),
            attempted: Family::default(),
            dropped: Family::default(),
            maintenance_failures: Counter::default(),
            refused: Family::default(),
            refreshes: Family::default(),
        };
        // Every queue and outcome is counted from zero, so an alert that
        // compares failures with deliveries finds a series for both.
        for channel in Channel::ALL {
            drop(metrics.dropped.get_or_create(&Dropped {
                queue: channel.as_str(),
            }));
            for outcome in Outcome::ALL {
                drop(metrics.attempted.get_or_create(&Attempted {
                    queue: channel.as_str(),
                    outcome: outcome.as_str(),
                }));
            }
        }
        metrics.registered(store)
    }

    fn registered(mut self, store: &Store) -> Self {
        let registry = &mut self.registry;
        registry.register(
            "transom_http_requests",
            "Requests answered, by method, matched route, and status",
            self.answered.clone(),
        );
        registry.register(
            "transom_http_request_duration_seconds",
            "How long each request took to answer, by method and matched route",
            self.durations.clone(),
        );
        registry.register(
            "transom_exchanges_created",
            "Exchanges sent, by kind, the sender's issuer, and how they were sent",
            self.sent.clone(),
        );
        registry.register(
            "transom_exchanges_ended",
            "Exchanges that ended, by kind and the state they ended in",
            self.ended.clone(),
        );
        registry.register(
            "transom_delivery_attempts",
            "Attempts to deliver a webhook, a push, or a wake-up, by outcome",
            self.attempted.clone(),
        );
        registry.register(
            "transom_deliveries_dropped",
            "Deliveries given up on after failing for as long as their queue keeps trying",
            self.dropped.clone(),
        );
        registry.register(
            "transom_maintenance_failures",
            "Maintenance passes that stopped on an error",
            self.maintenance_failures.clone(),
        );
        registry.register(
            "transom_signin_failures",
            "Requests whose sign-in failed, by reason and issuer",
            self.refused.clone(),
        );
        registry.register(
            "transom_session_refreshes",
            "Refreshes of sessions with their issuers, by outcome and issuer",
            self.refreshes.clone(),
        );
        registry.register(
            "transom_redis_errors",
            "Commands and connections to Redis that failed, by kind",
            store.errors.clone(),
        );
        registry.register(
            "transom_subscriptions",
            "Redis subscriptions this server holds open for waiting requests and live feeds",
            store.subscriptions.clone(),
        );
        self
    }

    pub(crate) fn answered(
        &self,
        request: &Method,
        route: Option<&str>,
        status: StatusCode,
        took: Duration,
    ) {
        let method = method(request);
        let route = route.unwrap_or_default().to_owned();
        self.durations
            .get_or_create(&Route {
                method,
                route: route.clone(),
            })
            .observe(took.as_secs_f64());
        self.answered
            .get_or_create(&Answered {
                method,
                route,
                status: status.as_u16(),
            })
            .inc();
    }

    pub(crate) fn sent(&self, exchange: &Exchange, issuer: &str, via: Via) {
        self.sent
            .get_or_create(&Sent {
                kind: kind(exchange.message.is_question()),
                issuer: issuer.to_owned(),
                via: via.as_str(),
            })
            .inc();
    }

    /// Counts an exchange that ended in `state`, whether it was a
    /// `question`.
    pub(crate) fn ended(&self, question: bool, state: State) {
        self.ended
            .get_or_create(&Ended {
                kind: kind(question),
                state: state.as_str(),
            })
            .inc();
    }

    pub(crate) fn attempted(&self, channel: Channel, outcome: Outcome) {
        self.attempted
            .get_or_create(&Attempted {
                queue: channel.as_str(),
                outcome: outcome.as_str(),
            })
            .inc();
    }

    pub(crate) fn dropped(&self, channel: Channel) {
        self.dropped
            .get_or_create(&Dropped {
                queue: channel.as_str(),
            })
            .inc();
    }

    pub(crate) fn maintenance_failed(&self) {
        self.maintenance_failures.inc();
    }

    pub(crate) fn refused(&self, reason: Refusal, issuer: &str) {
        self.refused
            .get_or_create(&Refused {
                reason: reason.as_str(),
                issuer: issuer.to_owned(),
            })
            .inc();
    }

    pub(crate) fn refreshed(&self, outcome: Refresh, issuer: &str) {
        self.refreshes
            .get_or_create(&Refreshed {
                outcome: outcome.as_str(),
                issuer: issuer.to_owned(),
            })
            .inc();
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::app::testing::TestApp;

    /// What `/metrics` answers for `app`.
    pub(crate) async fn scrape(app: &TestApp) -> String {
        let request = Request::get("/metrics").body(Body::empty()).unwrap();
        let response = super::scrape::router(app.app.clone())
            .oneshot(request)
            .await
            .unwrap();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    /// The value `text` gives `series`, a metric's name and its labels as
    /// `/metrics` writes them.
    pub(crate) fn sample<'a>(text: &'a str, series: &str) -> Option<&'a str> {
        text.lines()
            .find_map(|line| line.strip_prefix(series)?.strip_prefix(' '))
    }
}
