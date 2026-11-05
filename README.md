# transom

transom lets a program ask a person a question and get the answer back
without a chat. A question waits in an inbox, on the web and soon on an
Android phone, until someone replies; its answer returns to the program
through a waiting HTTP request, an event stream, an MCP tool, or a
signed webhook. transom also sends one-way notifications, like alerts.

[`plans/00-design.md`](plans/00-design.md) is the design, and
[`plans/13-the-http-api.md`](plans/13-the-http-api.md) is the whole API as
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
by `--config`; [`plans/03-sign-in.md`](plans/03-sign-in.md) describes the
issuers' settings. `TRANSOM_RETENTION`, 2 days by default, is how long
transom keeps an exchange after it ends, or a notification after it
arrives, before deleting it: seconds, or a duration such as `36h` or
`2d`. An open question is kept until it ends.

transom logs to stderr: one line for each request, with its path, status,
caller, and the reason for a failure, and one for each webhook and push it
delivers. It never logs a query, a header, a body, or a push endpoint.
`RUST_LOG=debug` adds the health checks.

## Developing

The tests run against a real Redis at `TRANSOM_TEST_REDIS_URL`, by default
`redis://127.0.0.1:6379`. `prek run --all-files` runs every check that CI
runs.
