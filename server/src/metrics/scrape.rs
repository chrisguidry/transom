//! `/metrics`, in the text format that Prometheus scrapes, on the listener
//! `TRANSOM_METRICS_LISTEN` names.

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::routing::get;
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::encoding::text::{encode_eof, encode_registry};
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;

use crate::app::App;

const OPENMETRICS: &str = "application/openmetrics-text; version=1.0.0; charset=utf-8";

/// Each sorted set whose members come due, by the `queue` label it is
/// reported under. The maintenance pass drains all of them.
const QUEUES: [(&str, &str); 5] = [
    ("webhook", "deliveries"),
    ("push", "pushes"),
    ("wake", "wakes"),
    ("repeat", "repeats"),
    ("expiry", "expiries"),
];

pub(crate) fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/metrics", get(metrics))
        .with_state(app)
}

async fn metrics(State(app): State<Arc<App>>) -> impl IntoResponse {
    // The queues are read first, so a failure to read them is in the
    // count of Redis errors written after. While Redis is down the queue
    // gauges are left out, rather than reported as empty, so that no alert
    // reads an outage as a drained queue.
    let queues = queues(&app).await;
    let mut text = String::new();
    let written = "a String takes whatever is written to it";
    encode_registry(&mut text, &app.metrics.registry).expect(written);
    if let Ok(queues) = queues {
        encode_registry(&mut text, &queues).expect(written);
    }
    encode_eof(&mut text).expect(written);
    ([(CONTENT_TYPE, OPENMETRICS)], text)
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Queue {
    queue: &'static str,
}

/// How many items each queue holds, and how long its earliest item has
/// been due, read from Redis now. An item being attempted, or waiting to
/// be tried again, is due in the future, so only work no server has taken
/// adds to the lag.
async fn queues(app: &App) -> redis::RedisResult<Registry> {
    let mut pipe = redis::pipe();
    for (_, key) in QUEUES {
        let key = app.store.key(key);
        pipe.zcard(&key).zrange_withscores(&key, 0, 0);
    }
    let mut connection = app.store.connection().await?;
    let answers: Vec<(u64, Vec<(String, f64)>)> = {
        let flat: Vec<redis::Value> = pipe.query_async(&mut connection).await?;
        flat.chunks(2)
            .map(|pair| {
                (
                    redis::from_redis_value_ref(&pair[0]).unwrap_or_default(),
                    redis::from_redis_value_ref(&pair[1]).unwrap_or_default(),
                )
            })
            .collect()
    };
    let now = crate::exchanges::records::now();
    let items = Family::<Queue, Gauge>::default();
    let lag = Family::<Queue, Gauge<f64, std::sync::atomic::AtomicU64>>::default();
    for ((queue, _), (count, earliest)) in QUEUES.into_iter().zip(answers) {
        let label = Queue { queue };
        items
            .get_or_create(&label)
            .set(i64::try_from(count).unwrap_or(i64::MAX));
        let due = earliest.first().map_or(f64::INFINITY, |(_, due)| *due);
        #[allow(clippy::cast_precision_loss)]
        let late = (now as f64 - due).max(0.0) / 1000.0;
        lag.get_or_create(&label).set(late);
    }
    let mut registry = Registry::default();
    registry.register(
        "transom_queue_items",
        "Items in each queue, read from Redis at each scrape",
        items,
    );
    registry.register_with_unit(
        "transom_queue_lag",
        "How long the earliest item in each queue has been due and not taken",
        prometheus_client::registry::Unit::Seconds,
        lag,
    );
    Ok(registry)
}
