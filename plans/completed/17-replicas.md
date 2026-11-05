# 17, Replicas

Closed 2026-11-05. Built, except where Open says otherwise.

The transom server must support multiple replicas for high
availability: several processes on one Redis, any of which can stop at
any time.

## What already worked across replicas

- Every change PUBLISHes to Redis, and each live feed and each
  `Prefer: wait` holds its own subscription, so a change written on one
  replica reaches a subscriber on another.
- The webhook signing key and the VAPID key are written once with
  `SET NX`, so every replica signs with the same key.
- Sessions and sign-in state are in Redis.
- Expiring and deleting an exchange are each one Lua script, so only one
  replica expires or deletes it, and announces it.
- The work every replica does at startup changes nothing when it runs
  again: registering `@transom` writes the same fields each time, and the
  retention backfill schedules deletions with `ZADD NX`.

## What had to change

- **MCP sessions.** The MCP service kept its sessions in the process, so
  a client whose next request reached another replica lost its session.
- **Claims were at most once.** The pass removed an item from its queue
  before it tried it, so a replica that stopped mid-attempt lost that
  webhook, push, or wake-up.
- **Creating an exchange and announcing it were two steps.** A replica
  that stopped between them left an exchange whose pushes, wake-ups, and
  webhooks were never queued.
- **Shutdown.** On SIGTERM a live feed held the server open until the pod
  was killed.
- **Redis.** transom connects to one Redis URL, which is enough for a
  Redis whose operator keeps a Service pointed at the current primary,
  as [redis-operator](https://github.com/chrisguidry/redis-operator)
  does. During a failover that Service refuses new connections for a
  few seconds and the open connections drop. A request after the drop
  failed, and a live feed ended, or, for a connection left open with no
  primary behind it, waited for messages that never came.

## Built

- **MCP without sessions.** `/mcp` runs rmcp's Streamable HTTP service
  with `legacy_session_mode` off and `NeverSessionManager`: every request
  is served on its own, so any replica answers it. The tools were
  already request and response, and a tool that waits still sends its
  progress notifications in the response's event stream.
- **Leases.** `server/src/queue.rs` is the queue every pass drains: a
  sorted set of ids by when each is due, and a hash per item that has
  fields. A replica claims a due item with a script that moves its score
  a lease into the future, a minute for webhooks, pushes, wake-ups, and
  the repeats of urgent exchanges, which is longer than any attempt with
  its timeouts. The replica then removes the item,
  or schedules it again with its attempts counted. A replica that stops
  mid-attempt does neither, so the item comes due when the lease runs
  out and any replica takes it. An item is attempted at least once, and
  never by two replicas at the same time while each attempt ends within
  its lease. A repeat is shown in one transaction that queues its pushes
  and wake-ups, counts it, publishes it, and removes it; a repeat of an
  exchange that is no longer open is dropped, and a recipient who read
  the exchange gets none. Every push, from an announcement, a repeat, a
  bulk read, or a deletion, takes its id from the exchange ids'
  generator, so pushes queued in one millisecond deliver in order.
- **One write for an exchange and its announcement.** Sending writes the
  exchange, its attachments, its published events, its scheduled
  deletion, its queued pushes, wake-ups, and webhooks, and for a sender
  not yet allowed, the hold and the request for consent, in one
  `MULTI`/`EXEC`. A keyed message also writes its replacement there: the
  server reads which open exchange the key names, announces that one's
  end and names it in the new exchange's own announcement, and the
  replacement script runs in the transaction. If another message with
  the key changed the index between the read and the write, the script
  replaces what the index then names, and the server announces that
  exchange's end after the write. A message let through after consent
  replaces and announces in one transaction the same way. What announcing reads, such as devices, members, and
  webhook URLs, it reads before that write. An invitation and its
  announcement are one write the same way.
- **Shutdown.** A signal cancels the app's `stopping` token. The pass
  claims nothing more, the live feeds and the exchange streams end, a
  waiting request returns what it has (`202` for a send), and a waiting
  MCP tool returns the exchange as it stands, so axum's graceful shutdown
  completes and each client reconnects to another replica. The server
  then waits up to five seconds for the attempt the pass already started,
  and exits; an attempt still running is left to its lease.
- **Redis failover.** The shared connection sends a command again, once,
  when its connection had dropped, which `ConnectionManager` replaces by
  itself; a command that timed out is not sent again. Reconnecting
  retries eight times, up to a second apart, which waits about five
  seconds for the new primary. A subscription pings its connection when
  it has been quiet for 15 seconds, and when its connection drops or
  stops answering, it subscribes again every half second until Redis
  takes it. A waiting request then checks again, and a live feed sends
  its whole list again, since it may have missed changes. If the
  reader's groups changed while it was down, the feed ends instead, as
  it does at any change of groups, and its client reconnects.

## Open

- A change after an exchange is created, such as a reply, an expiry, a
  withdrawal, or a decision on consent, is still written in one step and
  announced in another. A replica that stops between them leaves the
  change made but not pushed, woken, or sent to webhooks. A live feed
  and a waiting request read state, so they still see it, and the
  retention backfill schedules its deletion at the next start.
- When retention deletes an exchange, the pushes that close its
  notifications are queued after the script that deletes it, so a
  replica that stops between them leaves those browser notifications
  open.
- A live feed that resumes cannot tell its client about an exchange
  that was deleted while it was down; the client finds it gone at its
  next full read of the list.
