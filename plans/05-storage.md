# 05, Storage in Redis

Not built. Stub.

How identities, grants, and messages are kept in Redis.

## Decided

- Redis is the only store, and the tests run against a real Redis.
- The documentation tells operators to run Redis with append-only
  persistence, because a lost answer is the worst failure transom can
  have.
