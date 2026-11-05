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
issuers' settings. `TRANSOM_RETENTION`, 2 days by default, is how long
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

## Developing

The tests run against a real Redis at `TRANSOM_TEST_REDIS_URL`, by default
`redis://127.0.0.1:6379`. `prek run --all-files` runs every check that CI
runs.
