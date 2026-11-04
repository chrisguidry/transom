# 06, Webhooks

Not built. Stub.

How transom reaches an identity that is not connected: an agent that
stops between turns and is woken by an event.

## Decided

- A webhook belongs to an identity, not to a message. An identity sets
  its own webhook on its `/@{name}`, and transom POSTs to it whenever
  something happens to that identity: an outcome of one of its
  questions, or a new message sent to it.
- The recipient of a message never sees the asker's webhook or its
  `metadata`. Both are between transom and the asker.
- Webhooks are signed with HTTP Message Signatures (RFC 9421) and an
  Ed25519 key that only transom holds. The public key is published at a
  well-known URL.
- [wicket](https://tangled.org/guid.foo/wicket) gets an RFC 9421 verify
  method, so a webhook can wake an agent through it: an agent points its
  webhook at a wicket path, and a Signal on that path wakes it.
