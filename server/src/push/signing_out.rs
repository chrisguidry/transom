//! The last word to a session's devices when the session ends: each
//! browser gets a push that says it was signed out, and each phone a
//! wake-up, after which its sync finds the session over. Then the devices
//! are forgotten, so nothing more reaches a device no one is signed in on.

use serde_json::json;

use super::KEEP_ENDED;
use super::delivery::{self, Push};
use super::devices::{self, Wake};
use super::wakes;
use crate::app::{App, Failure};
use crate::exchanges::records;

/// A later push with this topic replaces one a push service still holds,
/// so a browser that signs in and out while offline hears it once.
const TOPIC: &str = "signed-out";

/// Tells each of `devices`, by identity and id, that the session with
/// `digest`, which has ended, was signed out, and forgets each. A device
/// that registered again under another session since is left alone.
pub(crate) async fn signed_out(
    app: &App,
    digest: &str,
    devices: &[(String, String)],
) -> Result<(), Failure> {
    let payload = json!({ "event": "signed-out" }).to_string();
    let push = Push {
        payload: &payload,
        urgency: "high",
        ttl: KEEP_ENDED,
        topic: TOPIC,
    };
    let now = records::now();
    let mut pipe = redis::pipe();
    pipe.atomic();
    for (identity, id) in devices {
        let Some(stored) = devices::stored(app, identity, id).await? else {
            continue;
        };
        if stored.session.as_deref() != Some(digest) {
            continue;
        }
        let device = (identity.as_str(), id.as_str());
        match &stored.device.wake {
            Wake::WebPush(subscription) => {
                delivery::enqueue_to(&mut pipe, app, device, subscription, &push, now);
            }
            Wake::Fcm(token) => wakes::enqueue_to(&mut pipe, app, device, token, now),
            // A phone that holds the live feed open hears its session end
            // when the feed closes.
            Wake::Connection(_) => {}
        }
        devices::forgetting(&mut pipe, app, identity, id);
    }
    let mut connection = app.store.connection().await?;
    pipe.query_async::<()>(&mut connection).await?;
    Ok(())
}
