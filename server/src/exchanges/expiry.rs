//! Expiring exchanges whose time to live has passed.

use std::sync::Arc;
use std::time::Duration;

use super::changes;
use super::records::{self, Found, State};
use crate::app::{App, Failure};
use crate::identities::records::register_transom;

/// How often transom looks for exchanges that have expired, which bounds
/// how late an expiry can be.
const EVERY: Duration = Duration::from_secs(1);

/// Expires every open or pending exchange due by `now`, and announces each.
/// Two servers sharing a Redis can both run this: the transition is atomic,
/// so each exchange expires, and is announced, once.
pub(crate) async fn expire_due(app: &App, now: i64) -> Result<usize, Failure> {
    let mut expired = 0;
    for id in changes::due(&app.store, now).await? {
        let (moved, _) = changes::transition(
            &app.store,
            &id,
            &[State::Open, State::Pending],
            State::Expired,
        )
        .await?;
        match (moved, records::load(&app.store, &id).await?) {
            (true, Found::Exchange(exchange)) => {
                super::announce(app, &exchange).await?;
                expired += 1;
            }
            // An exchange that ended some other way, or is gone, needs no
            // more looking at.
            _ => changes::forget_expiry(&app.store, &id).await?,
        }
    }
    Ok(expired)
}

/// One pass: registers `@transom`, if it is not yet, expires what is due
/// now, and attempts the webhook deliveries that are due. Registering here
/// rather than once at startup means transom starts while Redis is down
/// and registers as soon as Redis is back. A
/// failure is logged rather than stopping the passes, so a Redis outage
/// delays them instead of ending them.
pub(crate) async fn tick(app: &App) {
    let pass = async {
        register_transom(&app.store).await?;
        expire_due(app, records::now()).await?;
        crate::webhooks::deliver_due(app, records::now()).await
    };
    if let Err(error) = pass.await {
        tracing::warn!(%error, "the maintenance pass failed");
    }
}

/// Expires exchanges as they come due, for as long as the server runs. The
/// first pass runs at once, so exchanges that expired while no server was
/// running expire at startup.
pub(crate) async fn run(app: Arc<App>) {
    let mut ticks = tokio::time::interval(EVERY);
    loop {
        ticks.tick().await;
        tick(&app).await;
    }
}
