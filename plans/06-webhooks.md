# 06, Webhooks

Built in transom. wicket's RFC 9421 verify method is not built yet.

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

## Built

- An identity sets its webhook with `PATCH /@{name}` and
  `{"webhook": url}`, and clears it with an empty one. Only those who
  manage the identity see it.
- An exchange that opens is `received` by its recipient's webhook, and
  by each member's when the recipient is a group. One that ends has
  `ended` for its sender's webhook. Each POST is
  `{"event": ..., "exchange": ...}`, with the exchange as that identity
  sees it.
- The signature covers `@method`, `@target-uri`, `content-type`, and
  `content-digest` (RFC 9530, SHA-256), with the parameters `created`,
  `expires` five minutes later, `keyid`, and `alg="ed25519"`, under the
  label `transom`. The tests check the signing against RFC 9421's own
  Ed25519 example.
- The key is generated the first time a server needs it and kept in
  Redis, so every server on one Redis signs with one key. Its public
  half is a JWKS at `/.well-known/jwks.json`, named by its RFC 7638
  thumbprint.
- Deliveries wait in a queue in Redis that the once-a-second pass drains.
  A failed delivery is retried with a backoff that doubles from a second
  to an hour at most, and dropped after a day.
