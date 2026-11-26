//! Sessions: a person signed in through an issuer, held on the server so
//! a browser or the app carries only a random token.
//!
//! - `session:{digest}`, a hash of one session: its issuer, subject, name,
//!   refresh token, when it last refreshed and when the issuer last
//!   confirmed it, its kind, and a field `device:{identity}:{id}` for each
//!   device it registered. It is keyed by a digest of the token, so Redis
//!   holds nothing a client could present.
//! - `events:session:{digest}`, the channel that announces the session's
//!   end, which closes the event streams it opened.

use std::collections::HashMap;
use std::sync::LazyLock;

use aws_lc_rs::digest::{SHA256, digest};
use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use redis::{AsyncCommands, Script};
use serde::{Deserialize, Serialize};

use super::Verified;
use crate::app::{App, Failure};
use crate::metrics::Refusal;

/// How long a browser's session lasts without being used.
const BROWSER_LIFETIME: u64 = 30 * 24 * 60 * 60;

/// How long the app's session lasts without being used. A phone can go
/// months without asking transom anything while it still receives pushes,
/// and a person expects an installed app to stay signed in. The issuer
/// still decides: a session refreshes with it every fifteen minutes of
/// use, and ends when the issuer refuses. Forgetting the phone's device
/// ends it at once.
const APP_LIFETIME: u64 = 365 * 24 * 60 * 60;

/// Who holds a session's token, which sets how long it lasts unused, and
/// whether forgetting a device it registered ends it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    Browser,
    #[default]
    App,
}

impl Kind {
    fn lifetime(self) -> i64 {
        match self {
            Self::Browser => BROWSER_LIFETIME,
            Self::App => APP_LIFETIME,
        }
        .cast_signed()
    }

    fn stored(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::App => "app",
        }
    }
}

/// The session a request presented: the digest of its token, and who
/// holds it.
#[derive(Clone, Debug)]
pub(crate) struct Session {
    pub(crate) digest: String,
    pub(crate) kind: Kind,
}

/// The person an issuer signed in, as a new session records them.
pub(crate) struct SignedIn {
    /// The issuer's URL.
    pub(crate) issuer: String,
    pub(crate) subject: String,
    /// The value of the issuer's name claim, or empty.
    pub(crate) name: String,
    /// The issuer's refresh token, or empty when it gave none.
    pub(crate) refresh: String,
}

/// Why a session is over, as the 401 that answers it says.
pub(crate) struct Ended {
    pub(crate) refusal: Refusal,
    /// The name of the issuer that ended it, or `none`.
    pub(crate) issuer: String,
    pub(crate) reason: &'static str,
}

impl Ended {
    fn gone() -> Self {
        Self {
            refusal: Refusal::SessionEnded,
            issuer: "none".to_owned(),
            reason: "your session has ended; sign in again",
        }
    }
}

/// What a check of a session found.
pub(crate) enum Checked {
    Current(Verified),
    Ended(Ended),
}

pub(super) fn random() -> String {
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("the system can make random bytes");
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The base64url SHA-256 of `value`: how Redis keys a secret, and the
/// PKCE S256 challenge of a verifier.
pub(crate) fn hashed(value: &str) -> String {
    URL_SAFE_NO_PAD.encode(digest(&SHA256, value.as_bytes()))
}

pub(super) fn now() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

pub(super) fn key(app: &App, digest: &str) -> String {
    app.store.key(&format!("session:{digest}"))
}

/// The channel that announces the end of the session with `digest`.
pub(crate) fn channel(digest: &str) -> String {
    format!("events:session:{digest}")
}

/// The session hash's field for device `id` of `identity`.
fn device_field(identity: &str, id: &str) -> String {
    format!("device:{identity}:{id}")
}

/// Starts a session for `person`, and returns its token.
pub(crate) async fn start(app: &App, person: &SignedIn, kind: Kind) -> Result<String, Failure> {
    let token = random();
    let key = key(app, &hashed(&token));
    let mut connection = app.store.connection().await?;
    let mut pipe = redis::pipe();
    pipe.atomic()
        .hset_multiple(
            &key,
            &[
                ("issuer", person.issuer.as_str()),
                ("subject", &person.subject),
                ("name", &person.name),
                ("refresh", &person.refresh),
                ("refreshed", &now().to_string()),
                ("kind", kind.stored()),
            ],
        )
        .expire(&key, kind.lifetime());
    pipe.query_async::<()>(&mut connection).await?;
    Ok(token)
}

/// The caller of a request that carries the session `token`, as
/// [`check`] finds it, or the 401 that tells its client the session is
/// over.
pub(crate) async fn session(app: &App, token: &str) -> Result<Verified, Failure> {
    match check(app, &hashed(token)).await? {
        Checked::Current(verified) => Ok(verified),
        Checked::Ended(ended) => Err(super::refused(
            app,
            ended.refusal,
            &ended.issuer,
            super::unauthorized(Some("invalid_token"), &ended.reason),
        )),
    }
}

/// Checks the session with `digest`, refreshing it with its issuer when
/// it is due, and extends its life. A session whose issuer refuses the
/// refresh ends; one whose issuer cannot be reached carries on.
pub(crate) async fn check(app: &App, digest: &str) -> Result<Checked, Failure> {
    let mut connection = app.store.connection().await?;
    let key = key(app, digest);
    let fields: HashMap<String, String> = connection.hgetall(&key).await?;
    let (Some(issuer), Some(subject)) = (fields.get("issuer"), fields.get("subject")) else {
        return Ok(Checked::Ended(Ended::gone()));
    };
    if let Some(ended) = super::refreshing::when_due(app, digest, &fields).await? {
        return Ok(Checked::Ended(ended));
    }
    let kind = match fields.get("kind").map(String::as_str) {
        Some("app") => Kind::App,
        _ => Kind::Browser,
    };
    let () = connection.expire(&key, kind.lifetime()).await?;
    Ok(Checked::Current(Verified {
        issuer: issuer.clone(),
        subject: subject.clone(),
        name: fields.get("name").filter(|name| !name.is_empty()).cloned(),
        session: Some(Session {
            digest: digest.to_owned(),
            kind,
        }),
    }))
}

/// Whether the session with `digest` still exists, whatever its issuer
/// would say of it.
pub(crate) async fn exists(app: &App, digest: &str) -> redis::RedisResult<bool> {
    let mut connection = app.store.connection().await?;
    connection.exists(key(app, digest)).await
}

/// Lists a device in the session that registered it, unless the session
/// has ended, so a session that ends can reach its devices one last time.
static LIST_DEVICE: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        if redis.call('EXISTS', KEYS[1]) == 1 then
            redis.call('HSET', KEYS[1], ARGV[1], 1)
        end
        ",
    )
});

/// Lists device `id` of `identity` in the session with `digest`.
pub(crate) async fn list_device(
    app: &App,
    digest: &str,
    identity: &str,
    id: &str,
) -> redis::RedisResult<()> {
    let mut connection = app.store.connection().await?;
    LIST_DEVICE
        .key(key(app, digest))
        .arg(device_field(identity, id))
        .invoke_async(&mut connection)
        .await
}

/// Takes device `id` of `identity` off the session with `digest`'s list,
/// in `pipe`.
pub(crate) fn unlist_device(
    pipe: &mut redis::Pipeline,
    app: &App,
    digest: &str,
    identity: &str,
    id: &str,
) {
    pipe.hdel(key(app, digest), device_field(identity, id));
}

/// Deletes a session and returns what it held, or nothing when the
/// refresh token in `ARGV[1]`, if there is one, is no longer the
/// session's: then another refresh has already replaced it.
static END: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        if ARGV[1] ~= '' and redis.call('HGET', KEYS[1], 'refresh') ~= ARGV[1] then
            return false
        end
        local fields = redis.call('HGETALL', KEYS[1])
        redis.call('DEL', KEYS[1])
        return fields
        ",
    )
});

/// Ends the session whose token is `token`.
pub(crate) async fn end(app: &App, token: &str) -> Result<(), Failure> {
    end_digest(app, &hashed(token)).await
}

/// Ends the session whose token has the digest `digest`, as when the
/// device it registered is forgotten.
pub(crate) async fn end_digest(app: &App, digest: &str) -> Result<(), Failure> {
    ending(app, digest, "").await.map(drop)
}

/// Ends the session with `digest` if its refresh token is still `refresh`,
/// and returns whether it did.
pub(super) async fn end_refused(app: &App, digest: &str, refresh: &str) -> Result<bool, Failure> {
    ending(app, digest, refresh).await
}

/// Ends a session: deletes it, closes the event streams it opened, and
/// tells each device it registered that it was signed out, then forgets
/// the device. Everything that ends a session comes through here.
async fn ending(app: &App, digest: &str, unless_replaced: &str) -> Result<bool, Failure> {
    let mut connection = app.store.connection().await?;
    let ended: Option<HashMap<String, String>> = END
        .key(key(app, digest))
        .arg(unless_replaced)
        .invoke_async(&mut connection)
        .await?;
    let Some(fields) = ended else {
        return Ok(false);
    };
    let () = connection
        .publish(app.store.key(&channel(digest)), "ended")
        .await?;
    let devices: Vec<(String, String)> = fields
        .keys()
        .filter_map(|field| {
            let (identity, id) = field.strip_prefix("device:")?.split_once(':')?;
            Some((identity.to_owned(), id.to_owned()))
        })
        .collect();
    crate::push::signed_out(app, digest, &devices).await?;
    Ok(true)
}
