# transom

transom lets a program ask a person a question and get the answer back
without a chat. A question waits in an inbox, on the web and soon on an
Android phone, until someone replies; its answer returns to the program
through a waiting HTTP request, an event stream, an MCP tool, or a
signed webhook. transom also sends one-way notifications, like alerts.

[`plans/00-design.md`](plans/00-design.md) is the design, and
[`plans/completed/13-the-http-api.md`](plans/completed/13-the-http-api.md) is the whole API as
examples. The server, its web UI, and its MCP server are built; the
Android app is next.

[`android/`](android/README.md) is the Android app, and
[`desktop/`](desktop/README.md) is the app for a Linux desktop, with
its notifications and tray icon.

## Running it

transom needs Redis, run with append-only persistence: an answer lost
after someone gave it is the worst failure transom can have. It holds no
passwords or API keys, and trusts the token issuers its settings name.

```sh
docker run -p 8080:8080 \
  -e TRANSOM_REDIS_URL=redis://redis:6379 \
  -e TRANSOM_ISSUERS__AUTHENTIK__URL=https://auth.example.com/application/o/transom/ \
  -e TRANSOM_ISSUERS__AUTHENTIK__AUDIENCE=transom \
  -e TRANSOM_ISSUERS__AUTHENTIK__NAME_CLAIM=preferred_username \
  ghcr.io/chrisguidry/transom:<tag>
```

Settings come from `TRANSOM_` environment variables or a YAML file named
by `--config`; [`plans/completed/03-sign-in.md`](plans/completed/03-sign-in.md) describes the
issuers' settings. An issuer's `aliases` make some of its subjects sign
in as another subject's individual, for an agent that runs under several
workload identities. Each is `pattern=subject`, in a YAML list or
separated by spaces in the environment, and a pattern that ends in `*`
matches every subject that starts with the rest of it:
`TRANSOM_ISSUERS__CLUSTER__ALIASES='system:serviceaccount:agents:fixer--*=system:serviceaccount:agents:fixer'`.
The first alias that matches decides. `TRANSOM_RETENTION`, 2 days by default, is how long
transom keeps an exchange after it ends, or a notification after it
arrives, before deleting it: seconds, or a duration such as `36h` or
`2d`. An open question is kept until it ends. `TRANSOM_WATCHDOG_URL`
is a URL transom sends a GET to every `TRANSOM_WATCHDOG_INTERVAL`
(10 minutes by default) while it can reach Redis, for a dead man's
switch such as a [healthchecks.io](https://healthchecks.io) check.

Several transom processes can share one Redis and one
`TRANSOM_REDIS_PREFIX`, behind one load balancer, and any of them can
stop at any time; [`plans/completed/17-replicas.md`](plans/completed/17-replicas.md)
describes how.

### Adding transom to claude.ai

claude.ai's custom connectors sign a person in to transom's MCP server
through the issuer that transom's browser sign-in uses. transom answers
an MCP request without a token with a `401` whose `WWW-Authenticate`
header names its OAuth protected resource metadata, at
`/.well-known/oauth-protected-resource/mcp`, and that document names
every issuer with a `client_id`.

1. At the issuer, allow transom's client to redirect to
   `https://claude.ai/api/mcp/auth_callback` and
   `https://claude.com/api/mcp/auth_callback`, as well as transom's own
   `/login/callback`.
2. Set transom's `public_url`, for example
   `TRANSOM_PUBLIC_URL=https://transom.example.com`, and give the issuer
   its `client_id` and `client_secret`. The issuer issues the connector's
   access tokens to that client, so its `audience` must equal the
   `client_id`.
3. In claude.ai, add a custom connector with the URL
   `https://transom.example.com/mcp`. Under its advanced settings, enter
   the same client ID and client secret. Issuers such as Authentik do
   not offer dynamic client registration, so the connector cannot
   register a client of its own.

The connector then acts as the person who signs in, under the same
identity as their browser.

transom logs to stderr: one line for each request, with its path, status,
caller, and the reason for a failure, and one for each webhook and push it
delivers. It never logs a query, a header, a body, or a push endpoint.
`RUST_LOG=debug` adds the health checks.

### Metrics

`TRANSOM_METRICS_LISTEN`, such as `0.0.0.0:9090`, is an address where
transom serves Prometheus metrics at `/metrics`, in the OpenMetrics text
format. Without it, nothing serves them. The address is a listener of its
own, so the port that people reach never serves the metrics; keep it out
of any Service or proxy that the public reaches.

Each server counts what it did itself, so sum a counter across servers.
The queue gauges are read from Redis at each scrape, so every server
reports the same values and a query takes their `max`. While Redis is
down, the queue gauges are left out of the response.

| Series | Type | Labels |
|---|---|---|
| `transom_build_info` | gauge, always 1 | `version`: the release, or `dev` |
| `transom_http_requests_total` | counter | `method`, `route`, `status` |
| `transom_http_request_duration_seconds` | histogram | `method`, `route` |
| `transom_exchanges_created_total` | counter | `kind`, `issuer`, `via` |
| `transom_exchanges_ended_total` | counter | `kind`, `state` |
| `transom_delivery_attempts_total` | counter | `queue`, `outcome` |
| `transom_deliveries_dropped_total` | counter | `queue` |
| `transom_queue_items` | gauge | `queue` |
| `transom_queue_lag_seconds` | gauge | `queue` |
| `transom_maintenance_failures_total` | counter | none |
| `transom_signin_failures_total` | counter | `reason`, `issuer` |
| `transom_redis_errors_total` | counter | `kind` |
| `transom_subscriptions` | gauge | none |

The labels take these values:

- `method` is `GET`, `HEAD`, `POST`, `PUT`, `DELETE`, `PATCH`,
  `OPTIONS`, or `other`. `route` is the route the request matched, such
  as `/exchanges/{id}/reply`, or empty for a path no route matches.
  `status` is the response's status code. The duration of a request that
  waits for a reply includes the wait, and an event stream's ends when
  the stream starts.
- `kind` on the exchanges is `question` or `notification`. `issuer` is
  the issuer's name in the settings, such as `authentik` for
  `TRANSOM_ISSUERS__AUTHENTIK__URL`, or `none` when the caller's issuer
  is not known. `via` is `http` or `mcp`.
- `state` is `replied`, `withdrawn`, `expired`, `declined`, or
  `replaced`. The questions that `@transom` asks for consent are counted
  when they end, but they are not counted in
  `transom_exchanges_created_total`, which counts what senders send.
- `queue` on the deliveries is `webhook`, `push`, or `wake`. `outcome`
  is `delivered`; `refused`, an answer that trying again does not
  change; `unreachable`, no answer at all; `busy`, an answer asking to
  be tried again; or `forgotten`, an answer that the device no longer
  exists, which transom forgets. A dropped delivery failed for as long
  as its queue keeps trying: a day for a webhook, an hour for a push or
  a wake-up.
- `queue` on the gauges is `webhook`, `push`, `wake`, `repeat`, or
  `expiry`. The lag is how long the earliest item has been due without a
  server taking it, and stays at 0 while the maintenance pass keeps up.
- `reason` is `no_credentials`, `malformed`, `untrusted_issuer`,
  `invalid`, `issuer_unavailable`, `session_ended`, or `cross_site`.
- `kind` on the Redis errors is `timeout` or `connection`. Redis's own
  error replies are not counted.

With the prometheus-operator, a PodMonitor scrapes a container port
named `metrics`:

```yaml
apiVersion: monitoring.coreos.com/v1
kind: PodMonitor
metadata:
  name: transom
spec:
  selector:
    matchLabels:
      app: transom
  podMetricsEndpoints:
    - port: metrics
```

## Agent skills

`skills/` holds [Agent Skills](https://agentskills.io/) for agents that
use transom: `send-a-notification`, `ask-a-question`, `read-the-inbox`,
and `become-a-sender`. Each gives the MCP tool call and the same request
with curl, which reads the server's URL from `TRANSOM_URL` and the token
from `TRANSOM_TOKEN` or the file `TRANSOM_TOKEN_FILE` names. The
repository is its own plugin marketplace for Claude Code and Codex.

In Claude Code:

```sh
/plugin marketplace add chrisguidry/transom
/plugin install transom@transom
```

The skills are namespaced by the plugin's name, so asking a question is
`/transom:ask-a-question`.

In Codex, add the marketplace, then install `transom` from it:

```sh
codex plugin marketplace add chrisguidry/transom
codex plugin add transom@transom
```

Any other agent that reads the Agent Skills format can take `skills/`
directly, for example with `npx skills add https://github.com/chrisguidry/transom`.

## Developing

The tests run against a real Redis, which they start themselves with
Docker, so the only thing a machine needs is a Docker daemon that the
`docker` command reaches. Each test process starts one container from the
image that `test-redis/src/lib.rs` pins, on a port that Docker chooses,
and its tests share that Redis, each under a key prefix of its own. The
container is removed when the process exits, however it exits, so any
number of checkouts and worktrees can run the tests at once. Set
`TRANSOM_TEST_REDIS_URL` to run them against a Redis of your own instead.
The Android app's tests and `android/end-to-end.sh` get their Redis the
same way, from the `test-redis` binary, which prints a Redis URL and keeps
that Redis until its standard input closes:

```sh
cargo run -p test-redis    # Ctrl-D or Ctrl-C removes the container
```

`prek run --all-files` runs every check that CI runs.
