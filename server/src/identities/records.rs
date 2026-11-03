//! How identities are kept in Redis, under the store's prefix:
//!
//! - `names`, a hash from each name in use to the id of its identity.
//! - `former-names`, a hash from each name given up in a rename to the id
//!   of the identity that gave it up, which its old URLs redirect to.
//! - `bindings`, a hash from an issuer and subject to the id of the
//!   individual bound to them.
//! - `identity:{id}`, a hash of one identity's name and kind.
//! - `members:{id}`, the set of the ids of a group's members.
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

#[derive(Debug)]
pub(crate) struct Identity {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: Kind,
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
    let (name, kind): (String, String) = redis::cmd("HMGET")
        .arg(store.key(&format!("identity:{id}")))
        .arg("name")
        .arg("kind")
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
