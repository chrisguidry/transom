//! How exchanges are kept in Redis, under the store's prefix:
//!
//! - `exchange:{id}`, a hash of one exchange: its `sender` and `recipient`
//!   ids, its `state`, its `message` and `reply` as JSON, and when it was
//!   `created` and `expires`, in milliseconds since the epoch. A keyed
//!   exchange also has the name of its `keyed` index, and the id of the
//!   exchange it `replaces`, if it replaced one; an urgent one has how
//!   many `repeats` it has had.
//! - `sent:{identity}` and `received:{identity}`, sorted sets of exchange
//!   ids by when they were created.
//! - `expiries`, a sorted set of open exchange ids by when they expire.
//! - `deletions`, a sorted set of exchange ids by when retention deletes
//!   them, as `retention.rs` describes.
//! - `read:{identity}`, the set of exchanges an identity has read. Each
//!   reader is also a `reader:{identity}` field of the exchange's hash, so
//!   deleting the exchange finds a reader who has since left its group.
//! - `accepts:{identity}`, the set of senders an identity has allowed.
//! - `held:{recipient}:{sender}`, the exchanges held for the recipient's
//!   consent, and `consent:{recipient}`, a hash from each sender waiting
//!   on it to the exchange that asks.
//! - `keyed:{sender}:{recipient}:{key}`, the id of the latest exchange
//!   with that key to open, as `replacing.rs` describes.
//! - `repeats`, a sorted set of the repeats of urgent exchanges by when
//!   each is due, as `repeats.rs` describes.

use std::sync::LazyLock;

use redis::{AsyncCommands, Script};
use serde::{Deserialize, Serialize};

use super::message::Message;
use crate::store::Store;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum State {
    Pending,
    Open,
    Replied,
    Expired,
    Withdrawn,
    Declined,
    Replaced,
}

impl State {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            State::Pending => "pending",
            State::Open => "open",
            State::Replied => "replied",
            State::Expired => "expired",
            State::Withdrawn => "withdrawn",
            State::Declined => "declined",
            State::Replaced => "replaced",
        }
    }

    pub(crate) fn parse(state: &str) -> Option<Self> {
        Some(match state {
            "pending" => State::Pending,
            "open" => State::Open,
            "replied" => State::Replied,
            "expired" => State::Expired,
            "withdrawn" => State::Withdrawn,
            "declined" => State::Declined,
            "replaced" => State::Replaced,
            _ => return None,
        })
    }

    /// Whether the exchange has ended, so nothing more will change it.
    pub(crate) fn is_final(self) -> bool {
        !matches!(self, State::Pending | State::Open)
    }
}

/// The recipient's leg, stored with the id of whoever replied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Reply {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) choice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
    pub(crate) by: String,
    pub(crate) at: i64,
}

/// Why `@transom` sent an exchange, so that replying to it does what it
/// asked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "kind")]
pub(crate) enum Purpose {
    /// Asks the recipient whether `sender` may send to it.
    Consent { sender: String },
    /// Invites the recipient to join `group`.
    Invitation { group: String },
}

#[derive(Clone, Debug)]
pub(crate) struct Exchange {
    pub(crate) id: String,
    pub(crate) sender: String,
    pub(crate) recipient: String,
    pub(crate) state: State,
    pub(crate) message: Message,
    pub(crate) reply: Option<Reply>,
    pub(crate) created: i64,
    pub(crate) expires: Option<i64>,
    pub(crate) purpose: Option<Purpose>,
    /// The id of the earlier exchange with the same key that this one
    /// replaced.
    pub(crate) replaces: Option<String>,
    /// How many times an urgent exchange has been shown again.
    pub(crate) repeats: u32,
}

/// What is at an exchange's id.
pub(crate) enum Found {
    Exchange(Box<Exchange>),
    /// An exchange of a group that was deleted.
    Gone,
    Missing,
}

pub(crate) fn now() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

/// A new exchange id. The lists order exchanges created in the same
/// millisecond by id, so ids must increase within a millisecond too, or a
/// held message and the consent request made with it list in either order.
pub(crate) fn new_id() -> String {
    static IDS: std::sync::Mutex<ulid::Generator> = std::sync::Mutex::new(ulid::Generator::new());
    let mut ids = IDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    ids.generate()
        .expect("fewer than 2^80 exchanges are made in one millisecond")
        .to_string()
}

pub(crate) fn key(id: &str) -> String {
    format!("exchange:{id}")
}

/// Stores a new exchange and indexes it for its sender and recipient, in
/// `pipe`.
pub(crate) fn create(pipe: &mut redis::Pipeline, store: &Store, exchange: &Exchange) {
    let message = serde_json::to_string(&exchange.message).expect("a message serializes");
    pipe.hset_multiple(
        store.key(&key(&exchange.id)),
        &[
            ("sender", exchange.sender.as_str()),
            ("recipient", exchange.recipient.as_str()),
            ("state", exchange.state.as_str()),
            ("message", message.as_str()),
            ("created", &exchange.created.to_string()),
        ],
    )
    .zadd(
        store.key(&format!("sent:{}", exchange.sender)),
        &exchange.id,
        exchange.created,
    )
    .zadd(
        store.key(&format!("received:{}", exchange.recipient)),
        &exchange.id,
        exchange.created,
    );
    if let Some(expires) = exchange.expires {
        pipe.hset(store.key(&key(&exchange.id)), "expires", expires)
            .zadd(store.key("expiries"), &exchange.id, expires);
    }
    if let Some(purpose) = &exchange.purpose {
        let purpose = serde_json::to_string(purpose).expect("a purpose serializes");
        pipe.hset(store.key(&key(&exchange.id)), "purpose", purpose);
    }
}

pub(crate) async fn load(store: &Store, id: &str) -> redis::RedisResult<Found> {
    let mut connection = store.connection().await?;
    let mut fields: std::collections::HashMap<String, String> =
        connection.hgetall(store.key(&key(id))).await?;
    if fields.contains_key("gone") {
        return Ok(Found::Gone);
    }
    let mut take = |name: &str| fields.remove(name);
    let (Some(sender), Some(recipient), Some(state), Some(message), Some(created)) = (
        take("sender"),
        take("recipient"),
        take("state"),
        take("message"),
        take("created"),
    ) else {
        return Ok(Found::Missing);
    };
    // Fields transom wrote itself parse; a hand edit of Redis that breaks
    // one leaves a readable stand-in rather than an error.
    Ok(Found::Exchange(Box::new(Exchange {
        id: id.to_owned(),
        sender,
        recipient,
        state: State::parse(&state).unwrap_or(State::Open),
        message: serde_json::from_str(&message).unwrap_or_default(),
        reply: take("reply").and_then(|reply| serde_json::from_str(&reply).ok()),
        created: created.parse().unwrap_or_default(),
        expires: take("expires").and_then(|expires| expires.parse().ok()),
        purpose: take("purpose").and_then(|purpose| serde_json::from_str(&purpose).ok()),
        replaces: take("replaces"),
        repeats: take("repeats")
            .and_then(|repeats| repeats.parse().ok())
            .unwrap_or_default(),
    })))
}

/// The ids of the exchanges in any of `indexes`, newest first.
pub(crate) async fn list(
    store: &Store,
    indexes: &[String],
    limit: usize,
) -> redis::RedisResult<Vec<String>> {
    let mut connection = store.connection().await?;
    let mut ids: Vec<(String, f64)> = Vec::new();
    for index in indexes {
        let found: Vec<(String, f64)> = connection
            .zrevrange_withscores(store.key(index), 0, -1)
            .await?;
        ids.extend(found);
    }
    ids.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
    ids.dedup_by(|a, b| a.0 == b.0);
    Ok(ids.into_iter().take(limit).map(|(id, _)| id).collect())
}

pub(crate) async fn read_ids(
    store: &Store,
    identity: &str,
) -> redis::RedisResult<std::collections::HashSet<String>> {
    let mut connection = store.connection().await?;
    connection
        .smembers(store.key(&format!("read:{identity}")))
        .await
}

pub(crate) async fn is_read(store: &Store, identity: &str, id: &str) -> redis::RedisResult<bool> {
    let mut connection = store.connection().await?;
    connection
        .sismember(store.key(&format!("read:{identity}")), id)
        .await
}

/// Marks an exchange read or unread, unless the exchange is gone, so a mark
/// that races the exchange's deletion leaves nothing behind.
static SET_READ: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        if redis.call('EXISTS', KEYS[2]) == 0 then
            return 0
        end
        if ARGV[2] == '1' then
            redis.call('HSET', KEYS[2], 'reader:' .. ARGV[3], 1)
            return redis.call('SADD', KEYS[1], ARGV[1])
        end
        return redis.call('SREM', KEYS[1], ARGV[1])
        ",
    )
});

/// Marks exchange `id` read or unread for `identity`, and returns whether
/// that changed anything.
pub(crate) async fn set_read(
    store: &Store,
    identity: &str,
    id: &str,
    read: bool,
) -> redis::RedisResult<bool> {
    let mut connection = store.connection().await?;
    let changed: i64 = SET_READ
        .key(store.key(&format!("read:{identity}")))
        .key(store.key(&key(id)))
        .arg(id)
        .arg(if read { "1" } else { "0" })
        .arg(identity)
        .invoke_async(&mut connection)
        .await?;
    Ok(changed == 1)
}
