//! How identities are kept in Redis, under the store's prefix:
//!
//! - `names`, a hash from each name in use to the id of its identity.
//! - `former-names`, a hash from each name given up in a rename to the id
//!   of the identity that gave it up, which its old URLs redirect to.
//! - `bindings`, a hash from an issuer and subject to the id of the
//!   individual bound to them.
//! - `identity:{id}`, a hash of one identity's name and kind, and its
//!   webhook, display name, and the version of its picture, if it set them.
//! - `picture:{id}`, a hash of an identity's picture: its bytes, its type,
//!   and its version, which is the hash of its bytes.
//! - `members:{id}`, the set of the ids of a group's members.
//! - `groups:{id}`, the set of the ids of the groups an individual is in.
//!
//! Each change that touches more than one key runs as one Lua script, so
//! no request sees another's change half done.

use std::sync::LazyLock;

use redis::{AsyncCommands, Script};

use super::names::Name;
use crate::store::Store;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Individual,
    Group,
}

impl Kind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Kind::Individual => "individual",
            Kind::Group => "group",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Identity {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: Kind,
    pub(crate) display_name: Option<String>,
    /// The version of the identity's picture, if it has one.
    pub(crate) picture: Option<String>,
}

/// What a name refers to.
pub(crate) enum Lookup {
    Current(Identity),
    /// A name its identity gave up, with that identity as it is now.
    Former(Identity),
    Free,
}

fn binding(issuer: &str, subject: &str) -> String {
    serde_json::json!([issuer, subject]).to_string()
}

/// The id of the individual bound to `issuer` and `subject`.
pub(crate) async fn bound(
    store: &Store,
    issuer: &str,
    subject: &str,
) -> redis::RedisResult<Option<String>> {
    let mut connection = store.connection().await?;
    connection
        .hget(store.key("bindings"), binding(issuer, subject))
        .await
}

pub(crate) async fn identity(store: &Store, id: &str) -> redis::RedisResult<Identity> {
    let mut connection = store.connection().await?;
    let (name, kind, display_name, picture): (String, String, Option<String>, Option<String>) =
        redis::cmd("HMGET")
            .arg(store.key(&format!("identity:{id}")))
            .arg(&["name", "kind", "display_name", "picture"])
            .query_async(&mut connection)
            .await?;
    let kind = if kind == "group" {
        Kind::Group
    } else {
        Kind::Individual
    };
    Ok(Identity {
        id: id.to_owned(),
        name,
        kind,
        display_name,
        picture,
    })
}

pub(crate) async fn lookup(store: &Store, name: &Name) -> redis::RedisResult<Lookup> {
    let mut connection = store.connection().await?;
    let current: Option<String> = connection.hget(store.key("names"), name.as_str()).await?;
    if let Some(id) = current {
        return Ok(Lookup::Current(identity(store, &id).await?));
    }
    let former: Option<String> = connection
        .hget(store.key("former-names"), name.as_str())
        .await?;
    Ok(match former {
        Some(id) => Lookup::Former(identity(store, &id).await?),
        None => Lookup::Free,
    })
}

/// The ids of the groups `id` is a member of.
pub(crate) async fn groups_of(store: &Store, id: &str) -> redis::RedisResult<Vec<String>> {
    let mut connection = store.connection().await?;
    connection
        .smembers(store.key(&format!("groups:{id}")))
        .await
}

pub(crate) async fn is_member(store: &Store, group: &str, id: &str) -> redis::RedisResult<bool> {
    let mut connection = store.connection().await?;
    connection
        .sismember(store.key(&format!("members:{group}")), id)
        .await
}

/// What happened when a token claimed a name.
pub(crate) enum Claim {
    /// The name was in use by this identity, and the caller is the
    /// individual bound to the token, if any.
    Current { id: String, caller: Option<String> },
    /// The name is one the caller gave up, and this is the caller's id.
    Former(String),
    /// A new identity was created with this id: an individual for a token
    /// with none, or a group with the caller as its first member.
    Created(String),
}

/// Looks up the name and claims it in one step, so two requests racing
/// for a free name cannot both get it.
static CLAIM: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        local names, former, bindings, identity, members = KEYS[1], KEYS[2], KEYS[3], KEYS[4], KEYS[5]
        local name, binding, id = ARGV[1], ARGV[2], ARGV[3]
        local caller = redis.call('HGET', bindings, binding)
        local current = redis.call('HGET', names, name)
        if current then return {'current', current, caller or ''} end
        if caller and redis.call('HGET', former, name) == caller then
            return {'former', caller, caller}
        end
        redis.call('HSET', names, name, id)
        redis.call('HDEL', former, name)
        if caller then
            redis.call('HSET', identity, 'name', name, 'kind', 'group')
            redis.call('SADD', members, caller)
            redis.call('SADD', ARGV[4] .. caller, id)
        else
            redis.call('HSET', bindings, binding, id)
            redis.call('HSET', identity, 'name', name, 'kind', 'individual')
        end
        return {'created', id, caller or id}
        ",
    )
});

pub(crate) async fn claim(
    store: &Store,
    name: &Name,
    issuer: &str,
    subject: &str,
) -> redis::RedisResult<Claim> {
    let id = ulid::Ulid::generate().to_string();
    let mut connection = store.connection().await?;
    let (status, found, caller): (String, String, String) = CLAIM
        .key(store.key("names"))
        .key(store.key("former-names"))
        .key(store.key("bindings"))
        .key(store.key(&format!("identity:{id}")))
        .key(store.key(&format!("members:{id}")))
        .arg(name.as_str())
        .arg(binding(issuer, subject))
        .arg(&id)
        .arg(store.key("groups:"))
        .invoke_async(&mut connection)
        .await?;
    Ok(match status.as_str() {
        "current" => Claim::Current {
            id: found,
            caller: Some(caller).filter(|caller| !caller.is_empty()),
        },
        "former" => Claim::Former(found),
        _ => Claim::Created(found),
    })
}

static RENAME: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        if redis.call('HEXISTS', KEYS[1], ARGV[2]) == 1 then return 0 end
        redis.call('HDEL', KEYS[1], ARGV[1])
        redis.call('HSET', KEYS[1], ARGV[2], ARGV[3])
        redis.call('HDEL', KEYS[2], ARGV[2])
        redis.call('HSET', KEYS[2], ARGV[1], ARGV[3])
        redis.call('HSET', KEYS[3], 'name', ARGV[2])
        return 1
        ",
    )
});

/// Renames `identity` to `name`, leaving its old name to redirect. Returns
/// `false` if the name is taken.
pub(crate) async fn rename(
    store: &Store,
    identity: &Identity,
    name: &Name,
) -> redis::RedisResult<bool> {
    let mut connection = store.connection().await?;
    RENAME
        .key(store.key("names"))
        .key(store.key("former-names"))
        .key(store.key(&format!("identity:{}", identity.id)))
        .arg(&identity.name)
        .arg(name.as_str())
        .arg(&identity.id)
        .invoke_async(&mut connection)
        .await
}

/// The id and the name of transom's own identity, which sends consent
/// requests and invitations. It is registered when the server starts, so
/// the name is always taken, and no token is bound to it, so no one can
/// sign in as it.
pub(crate) const TRANSOM: &str = "transom";

pub(crate) async fn register_transom(store: &Store) -> redis::RedisResult<()> {
    let mut connection = store.connection().await?;
    let mut pipe = redis::pipe();
    pipe.atomic()
        .hset_nx(store.key("names"), TRANSOM, TRANSOM)
        .hset_multiple(
            store.key(&format!("identity:{TRANSOM}")),
            &[("name", TRANSOM), ("kind", "individual")],
        );
    pipe.query_async(&mut connection).await
}

pub(crate) async fn members(store: &Store, group: &str) -> redis::RedisResult<Vec<String>> {
    let mut connection = store.connection().await?;
    connection
        .smembers(store.key(&format!("members:{group}")))
        .await
}

pub(crate) async fn add_member(store: &Store, group: &str, id: &str) -> redis::RedisResult<()> {
    let mut connection = store.connection().await?;
    let mut pipe = redis::pipe();
    pipe.atomic()
        .sadd(store.key(&format!("members:{group}")), id)
        .sadd(store.key(&format!("groups:{id}")), group);
    pipe.query_async(&mut connection).await
}

/// Removes `id` from `group`, and returns how many members are left.
pub(crate) async fn remove_member(
    store: &Store,
    group: &str,
    id: &str,
) -> redis::RedisResult<usize> {
    let mut connection = store.connection().await?;
    let members = store.key(&format!("members:{group}"));
    let mut pipe = redis::pipe();
    pipe.atomic()
        .srem(&members, id)
        .ignore()
        .srem(store.key(&format!("groups:{id}")), group)
        .ignore()
        .scard(&members);
    let (left,): (usize,) = pipe.query_async(&mut connection).await?;
    Ok(left)
}

/// Deletes a group with no members left, freeing its name and every
/// former name of its.
pub(crate) async fn delete_group(store: &Store, group: &Identity) -> redis::RedisResult<()> {
    let mut connection = store.connection().await?;
    let former: std::collections::HashMap<String, String> =
        connection.hgetall(store.key("former-names")).await?;
    let mut pipe = redis::pipe();
    pipe.atomic()
        .hdel(store.key("names"), &group.name)
        .del(store.key(&format!("identity:{}", group.id)))
        .del(store.key(&format!("picture:{}", group.id)))
        .del(store.key(&format!("members:{}", group.id)));
    for (name, _) in former.iter().filter(|(_, id)| **id == group.id) {
        pipe.hdel(store.key("former-names"), name);
    }
    pipe.query_async(&mut connection).await
}

/// The URL transom sends an identity's events to, if it set one.
pub(crate) async fn webhook(store: &Store, id: &str) -> redis::RedisResult<Option<String>> {
    let mut connection = store.connection().await?;
    connection
        .hget(store.key(&format!("identity:{id}")), "webhook")
        .await
}

pub(crate) async fn set_webhook(
    store: &Store,
    id: &str,
    url: Option<&str>,
) -> redis::RedisResult<()> {
    let mut connection = store.connection().await?;
    let key = store.key(&format!("identity:{id}"));
    match url {
        Some(url) => connection.hset(key, "webhook", url).await,
        None => connection.hdel(key, "webhook").await,
    }
}

/// Sets or, with `None`, removes the name an identity is shown by.
pub(crate) async fn set_display_name(
    store: &Store,
    id: &str,
    display_name: Option<&str>,
) -> redis::RedisResult<()> {
    let mut connection = store.connection().await?;
    let key = store.key(&format!("identity:{id}"));
    match display_name {
        Some(display_name) => connection.hset(key, "display_name", display_name).await,
        None => connection.hdel(key, "display_name").await,
    }
}
