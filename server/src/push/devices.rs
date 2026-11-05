//! An identity's devices, at `/@{name}/devices/{id}`: what gets its
//! notifications. A device chooses its own id, and PUTs its name and how
//! to wake it. Only the identity itself lists, adds, or removes them.
//!
//! - `devices:{identity}`, a hash of device id to the device as JSON,
//!   with the digest of the app session that registered it, if one did.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header::LOCATION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::encryption::{Receiver, encrypt};
use crate::app::{App, Failure};
use crate::identities::records::Identity;
use crate::identities::{Resolved, caller, resolve};
use crate::signin::{Verified, sessions};

type Outcome = Result<Response, Failure>;

/// A device: a name to tell it by, and how to wake it.
#[derive(Deserialize, Serialize)]
pub(crate) struct Device {
    #[serde(default)]
    pub(crate) name: String,
    pub(crate) wake: Wake,
}

/// A device as Redis holds it. The session is kept apart from what the
/// device PUT, so a request cannot name a session for its device to end.
#[derive(Deserialize, Serialize)]
struct Stored {
    #[serde(flatten)]
    device: Device,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session: Option<String>,
}

/// How transom reaches a device.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Wake {
    /// A browser's push subscription, as `PushSubscription.toJSON()`
    /// writes it.
    WebPush(Subscription),
    /// `true`: a phone that holds the live feed open while it runs in the
    /// background, and so is never pushed to.
    Connection(bool),
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Subscription {
    pub(crate) endpoint: String,
    pub(crate) keys: Keys,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Keys {
    pub(crate) p256dh: String,
    pub(crate) auth: String,
}

impl Subscription {
    /// The browser's public key and authentication secret, decoded.
    pub(crate) fn secrets(&self) -> Option<(Vec<u8>, Vec<u8>)> {
        let public = URL_SAFE_NO_PAD
            .decode(self.keys.p256dh.trim_end_matches('='))
            .ok()?;
        let auth = URL_SAFE_NO_PAD
            .decode(self.keys.auth.trim_end_matches('='))
            .ok()?;
        (public.len() == 65 && auth.len() == 16).then_some((public, auth))
    }

    /// Whether transom can push to it: a web endpoint, and a key that an
    /// encrypted message can be made for.
    fn reachable(&self) -> bool {
        let web = url::Url::parse(&self.endpoint)
            .is_ok_and(|url| url.scheme() == "https" || url.scheme() == "http");
        web && self.secrets().is_some_and(|(public, auth)| {
            encrypt(
                b"",
                &Receiver {
                    public: &public,
                    auth: &auth,
                },
            )
            .is_ok()
        })
    }
}

fn key(identity: &str) -> String {
    format!("devices:{identity}")
}

/// Every device of `identity`, by id.
pub(crate) async fn of(app: &App, identity: &str) -> redis::RedisResult<Vec<(String, Device)>> {
    let mut connection = app.store.connection().await?;
    let stored: Vec<(String, String)> = connection.hgetall(app.store.key(&key(identity))).await?;
    Ok(stored
        .into_iter()
        .filter_map(|(id, json)| Some((id, serde_json::from_str::<Stored>(&json).ok()?.device)))
        .collect())
}

/// One device of `identity`, if it still has it.
pub(crate) async fn one(app: &App, identity: &str, id: &str) -> redis::RedisResult<Option<Device>> {
    let mut connection = app.store.connection().await?;
    let stored: Option<String> = connection.hget(app.store.key(&key(identity)), id).await?;
    Ok(stored
        .and_then(|json| serde_json::from_str::<Stored>(&json).ok())
        .map(|stored| stored.device))
}

/// Forgets a device, as when its push service says it is gone.
pub(crate) async fn forget(app: &App, identity: &str, id: &str) -> redis::RedisResult<()> {
    let mut connection = app.store.connection().await?;
    connection.hdel(app.store.key(&key(identity)), id).await
}

/// The identity a path names, if the caller is that identity.
async fn own(
    app: &App,
    path: &str,
    verified: &Verified,
) -> Result<Result<Identity, Response>, Failure> {
    let identity = match resolve(app, path).await? {
        Resolved::Current(identity) => identity,
        Resolved::Moved(response) => return Ok(Err(response)),
    };
    if caller(app, verified).await?.id != identity.id {
        return Err(Failure::new(
            StatusCode::FORBIDDEN,
            format!("only @{} may manage its devices", identity.name),
        ));
    }
    Ok(Ok(identity))
}

/// The devices, and the key a browser subscribes with; or, for a browser,
/// the page that lists them and pairs a phone.
pub(crate) async fn list(
    State(app): State<Arc<App>>,
    Path(path): Path<String>,
    headers: HeaderMap,
    verified: Verified,
) -> Outcome {
    let identity = match own(&app, &path, &verified).await? {
        Ok(identity) => identity,
        Err(moved) => return Ok(moved),
    };
    if crate::web::wants_html(&headers) {
        let devices = of(&app, &identity.id).await?;
        return crate::web::pages::devices(&identity.name, devices, app.origin(&headers));
    }
    let key = app.push.vapid(&app).await?.public_key();
    let devices: Vec<_> = of(&app, &identity.id)
        .await?
        .into_iter()
        .map(|(id, device)| json!({ "id": id, "name": device.name, "wake": device.wake }))
        .collect();
    Ok(Json(json!({ "key": key, "devices": devices })).into_response())
}

/// Adds a device under the id it chose, or replaces it, as when a browser
/// subscribes again.
pub(crate) async fn put(
    State(app): State<Arc<App>>,
    Path((path, id)): Path<(String, String)>,
    verified: Verified,
    Json(device): Json<Device>,
) -> Outcome {
    let identity = match own(&app, &path, &verified).await? {
        Ok(identity) => identity,
        Err(moved) => return Ok(moved),
    };
    let fair_id = (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    let wakes = match &device.wake {
        Wake::WebPush(subscription) => subscription.reachable(),
        Wake::Connection(held) => *held,
    };
    if !fair_id || !wakes {
        return Err(Failure::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "a device has an id of up to 64 letters, digits, - and _, and wakes by web_push: \
             an http or https endpoint, a P-256 p256dh key, and a 16-octet auth secret, \
             or by connection: true",
        ));
    }
    let stored = Stored {
        device,
        session: verified.session,
    };
    let json = serde_json::to_string(&stored).expect("a device serializes");
    let mut connection = app.store.connection().await?;
    let added: i64 = connection
        .hset(app.store.key(&key(&identity.id)), &id, json)
        .await?;
    let status = if added == 1 {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    let location = format!("/@{}/devices/{id}", identity.name);
    Ok((
        status,
        [(LOCATION, location)],
        Json(json!({ "id": id, "name": stored.device.name })),
    )
        .into_response())
}

/// Forgets a device, as when a browser unsubscribes or a phone is lost,
/// and ends the app session that registered it.
pub(crate) async fn remove(
    State(app): State<Arc<App>>,
    Path((path, id)): Path<(String, String)>,
    verified: Verified,
) -> Outcome {
    let identity = match own(&app, &path, &verified).await? {
        Ok(identity) => identity,
        Err(moved) => return Ok(moved),
    };
    let mut connection = app.store.connection().await?;
    let stored: Option<String> = connection
        .hget(app.store.key(&key(&identity.id)), &id)
        .await?;
    forget(&app, &identity.id, &id).await?;
    let session = stored
        .and_then(|json| serde_json::from_str::<Stored>(&json).ok())
        .and_then(|stored| stored.session);
    if let Some(session) = session {
        sessions::end_digest(&app, &session).await?;
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}
