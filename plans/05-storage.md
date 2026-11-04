# 05, Storage in Redis

Built for identities and exchanges.

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
  a sorted set of expiry times, a set of what each identity has read, and
  the sets and lists that consent and invitations need. Each change that
  touches more than one key is a Lua script.
