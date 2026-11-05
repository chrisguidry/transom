# 05, Storage in Redis

Closed 2026-11-05. Built for identities and exchanges.

How identities, grants, and messages are kept in Redis.

## Decided

- Redis is the only store, and the tests run against a real Redis.
- Attachments are stored in Redis with a size limit. When video comes,
  attachments move to an optional S3-compatible object store, which
  also lifts the limit.
- The documentation tells operators to run Redis with append-only
  persistence, because a lost answer is the worst failure transom can
  have.

## Built

- Every key starts with the `redis_prefix` setting, `transom` by default,
  so transom can share a Redis, and each test has keys of its own.
- One connection, made on first use and shared, reconnects by itself,
  with two retries at most, so a request fails in under a second when
  Redis is down. transom starts even when Redis is down, and its health
  check, which uses a connection of its own, reports it.
- Identities are kept in the hashes `names`, `former-names`, and
  `bindings`, a hash for each identity, and a set of each group's members.
- Exchanges are kept as `server/src/exchanges/records.rs` describes: a hash
  for each exchange, sorted sets of what each identity sent and received,
  a sorted set of expiry times, a sorted set of deletion times, a set of
  what each identity has read, and the sets and lists that consent and
  invitations need. Each change that touches more than one key is a Lua
  script.
- Exchanges are kept for the `retention` setting, 2 days by default,
  after they end, or for a notification, after it arrives, and then
  deleted with every key that names them, as plan 04 describes. An open
  question is kept until it ends. The setting is seconds or a duration
  such as `2d`, in `TRANSOM_RETENTION` or the YAML file's `retention`.
  transom deletes in its once-a-second pass rather than with Redis's key
  expiry, because an exchange is several keys and its id is in other
  identities' indexes, which a key's own expiry cannot reach.
