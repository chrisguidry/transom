//! The changes an exchange goes through after it is sent, each one a Lua
//! script so that two requests racing to change one exchange cannot both
//! succeed, and the events each change publishes.

use std::sync::LazyLock;

use redis::{AsyncCommands, Script};

use super::records::{Exchange, Reply, State, key};
use crate::store::Store;

/// The channel that announces each change to one exchange.
pub(crate) fn exchange_channel(id: &str) -> String {
    format!("events:exchange:{id}")
}

/// The channel that announces each change to any exchange an identity sent
/// or received, and each group an individual joins or leaves, as a
/// [`Notice`].
pub(crate) fn identity_channel(identity: &str) -> String {
    format!("events:identity:{identity}")
}

/// What a message on an identity's channel announces.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Notice {
    /// The exchange with this id changed, or the identity's read state on
    /// it did.
    Exchange(String),
    /// The individual joined or left a group.
    Groups,
}

/// The payload of [`Notice::Groups`]. Exchange ids are ULIDs, 26
/// characters of uppercase Crockford base32, so this lowercase word is
/// never one.
const GROUPS: &str = "groups";

impl Notice {
    pub(crate) fn parse(payload: String) -> Self {
        if payload == GROUPS {
            Self::Groups
        } else {
            Self::Exchange(payload)
        }
    }
}

/// Announces that `individual` joined or left a group, on its own channel.
/// A live feed subscribes to its groups' channels when it opens, so this
/// tells the feed those subscriptions are out of date.
pub(crate) async fn publish_groups(store: &Store, individual: &str) -> redis::RedisResult<()> {
    let mut connection = store.connection().await?;
    connection
        .publish(store.key(&identity_channel(individual)), GROUPS)
        .await
}

/// Announces that `exchange` changed, to anyone waiting on it and to its
/// sender's and recipient's feeds, in `pipe`.
pub(crate) fn publish(pipe: &mut redis::Pipeline, store: &Store, exchange: &Exchange) {
    pipe.publish(
        store.key(&exchange_channel(&exchange.id)),
        exchange.state.as_str(),
    )
    .publish(store.key(&identity_channel(&exchange.sender)), &exchange.id)
    .publish(
        store.key(&identity_channel(&exchange.recipient)),
        &exchange.id,
    );
}

/// Announces that `reader` marked exchange `id` read or unread, to the
/// reader's own feeds. Read state is private, so the exchange's channel
/// and its sender's and group's channels never hear it.
pub(crate) async fn publish_read(store: &Store, reader: &str, id: &str) -> redis::RedisResult<()> {
    let mut connection = store.connection().await?;
    connection
        .publish(store.key(&identity_channel(reader)), id)
        .await
}

/// Marks each exchange read for one reader, and announces each one it
/// changes on the reader's channel, as `read_all` describes.
static READ_ALL: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        local changed = {}
        for i = 2, #KEYS do
            local id = ARGV[i + 1]
            if redis.call('EXISTS', KEYS[i]) == 1 and redis.call('SADD', KEYS[1], id) == 1 then
                redis.call('HSET', KEYS[i], 'reader:' .. ARGV[1], 1)
                redis.call('PUBLISH', ARGV[2], id)
                changed[#changed + 1] = id
            end
        end
        return changed
        ",
    )
});

/// Marks each of `ids` read for `reader` in one step, and announces each
/// it changed to the reader's own feeds, as [`publish_read`] does for one.
/// Returns the ids it changed: one already read, or gone, is left out. A
/// single read that races this changes each exchange once between them,
/// so each is announced and pushed once.
pub(crate) async fn read_all(
    store: &Store,
    reader: &str,
    ids: &[&str],
) -> redis::RedisResult<Vec<String>> {
    let mut connection = store.connection().await?;
    let mut invocation = READ_ALL.prepare_invoke();
    invocation
        .key(store.key(&format!("read:{reader}")))
        .arg(reader)
        .arg(store.key(&identity_channel(reader)));
    for id in ids {
        invocation.key(store.key(&key(id))).arg(*id);
    }
    invocation.invoke_async(&mut connection).await
}

/// What happened when someone replied.
pub(crate) enum Replying {
    /// This reply is the exchange's reply.
    Accepted,
    /// The exchange was not open: it has this state, and this reply if it
    /// was already replied to.
    Refused(State, Option<Reply>),
}

static REPLY: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        local state = redis.call('HGET', KEYS[1], 'state')
        if state ~= 'open' then
            return {state, redis.call('HGET', KEYS[1], 'reply') or ''}
        end
        redis.call('HSET', KEYS[1], 'state', 'replied', 'reply', ARGV[1])
        redis.call('ZREM', KEYS[2], ARGV[2])
        return {'accepted', ''}
        ",
    )
});

pub(crate) async fn reply(store: &Store, id: &str, reply: &Reply) -> redis::RedisResult<Replying> {
    let mut connection = store.connection().await?;
    let (status, existing): (String, String) = REPLY
        .key(store.key(&key(id)))
        .key(store.key("expiries"))
        .arg(serde_json::to_string(reply).expect("a reply serializes"))
        .arg(id)
        .invoke_async(&mut connection)
        .await?;
    Ok(match State::parse(&status) {
        Some(state) => Replying::Refused(state, serde_json::from_str(&existing).ok()),
        None => Replying::Accepted,
    })
}

/// Moves an exchange from one of `from` to `to`, unless it has already
/// moved elsewhere, and returns the state it was in.
static MOVE: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        local state = redis.call('HGET', KEYS[1], 'state')
        for i = 3, #ARGV do
            if state == ARGV[i] then
                redis.call('HSET', KEYS[1], 'state', ARGV[1])
                redis.call('ZREM', KEYS[2], ARGV[2])
                return state
            end
        end
        return state or ''
        ",
    )
});

/// Moves the exchange to `to` if it is in one of `from`. Returns whether it
/// moved, and the state it was in, which is `None` if it does not exist.
pub(crate) async fn transition(
    store: &Store,
    id: &str,
    from: &[State],
    to: State,
) -> redis::RedisResult<(bool, Option<State>)> {
    let mut connection = store.connection().await?;
    let mut invocation = MOVE.prepare_invoke();
    invocation
        .key(store.key(&key(id)))
        .key(store.key("expiries"))
        .arg(to.as_str())
        .arg(id);
    for state in from {
        invocation.arg(state.as_str());
    }
    let was: String = invocation.invoke_async(&mut connection).await?;
    let was = State::parse(&was);
    Ok((was.is_some_and(|was| from.contains(&was)), was))
}

/// The exchanges whose time to live has passed by `now`.
pub(crate) async fn due(store: &Store, now: i64) -> redis::RedisResult<Vec<String>> {
    let mut connection = store.connection().await?;
    connection
        .zrangebyscore(store.key("expiries"), "-inf", now)
        .await
}

/// Takes an exchange out of the expiry index.
pub(crate) async fn forget_expiry(store: &Store, id: &str) -> redis::RedisResult<()> {
    let mut connection = store.connection().await?;
    connection.zrem(store.key("expiries"), id).await
}

/// Deletes every exchange a deleted group received, leaving a tombstone so
/// each one's URL answers `410 Gone`, and announces each, so anything
/// streaming one of them learns it is gone.
pub(crate) async fn bury(store: &Store, group: &str) -> redis::RedisResult<()> {
    let mut connection = store.connection().await?;
    let received = store.key(&format!("received:{group}"));
    let ids: Vec<String> = connection.zrange(&received, 0, -1).await?;
    let mut pipe = redis::pipe();
    pipe.atomic();
    for id in &ids {
        pipe.del(store.key(&key(id)))
            .del(store.key(&super::attachments::key(id)))
            .hset(store.key(&key(id)), "gone", 1)
            .zrem(store.key("expiries"), id)
            .publish(store.key(&exchange_channel(id)), "gone")
            .publish(store.key(&identity_channel(group)), id);
    }
    pipe.del(&received)
        .del(store.key(&format!("accepts:{group}")))
        .del(store.key(&format!("consent:{group}")));
    pipe.query_async(&mut connection).await
}
