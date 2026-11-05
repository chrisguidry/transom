# 17, Replicas

Not built.

The transom server must support multiple replicas for high
availability: several processes on one Redis, any of which can stop at
any time.

## What already works across replicas

- Every change PUBLISHes to Redis, and each live feed and each
  `Prefer: wait` holds its own subscription, so a change written on one
  replica reaches a subscriber on another.
- The once-a-second pass claims each expiry, deletion, webhook, Web
  Push, and wake in Redis, so two replicas never act on the same item.
- The webhook signing key and the VAPID key are written once with
  `SET NX`, so every replica signs with the same key.
- Sessions and sign-in state are in Redis.

## What has to change

- **MCP sessions.** The MCP service keeps its sessions in the process
  (`LocalSessionManager`), so a client whose next request reaches
  another replica loses its session. The tools are request and
  response, so the service can run without sessions.
- **Claims are at most once.** The pass removes an item from its queue
  before it tries it, so a replica that stops mid-attempt loses that
  webhook, push, or wake. A claim should be a lease that another
  replica takes over when it expires.
- **Creating an exchange and announcing it are two steps.** A replica
  that stops between them leaves an exchange whose pushes, wakes, and
  webhooks are never queued. They should be written in one step.
- **Shutdown.** On SIGTERM the server should stop taking work, finish
  or hand back what it claimed, and close its live feeds, so clients
  reconnect to another replica at once instead of when the pod is
  killed.
- **Redis.** transom connects to one Redis URL, which is enough for a
  Redis whose operator keeps a Service pointed at the current primary,
  as [redis-operator](https://github.com/chrisguidry/redis-operator)
  does. During a failover that Service refuses new connections for a
  few seconds, so every Redis call has to survive a refused or dropped
  connection by reconnecting, and the live feeds by resubscribing.
