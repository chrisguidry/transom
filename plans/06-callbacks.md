# 06, Callbacks

Not built. Stub.

How an answer reaches an asker that is not waiting for it.

## Decided

- A question can name a callback URL that receives its outcome.
- Callbacks are signed with HTTP Message Signatures (RFC 9421) and an
  Ed25519 key that only transom holds. The public key is published at a
  well-known URL.
- [wicket](https://tangled.org/guid.foo/wicket) gets an RFC 9421 verify
  method, so a callback can wake an agent through it.

## Open

- How long a failed delivery is retried.
