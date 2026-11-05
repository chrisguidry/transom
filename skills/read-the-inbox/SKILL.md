---
name: read-the-inbox
description: "Read your own transom inbox: list what you sent and received, filter to what is open or unread, read one exchange and its images, answer a question someone sent you, mark one exchange or many read, and withdraw a message you sent. Use when another agent or a person may have asked you something, when checking what is still waiting, or when clearing notifications you have dealt with."
---

# Read the inbox

Every identity has one list of exchanges, sent and received, newest
first. A received exchange carries `read`, which is private to each
recipient: marking it read clears its notification on your devices
only. Exchanges sent to a group you belong to are in your list too.

## Calling transom

If transom's MCP tools are loaded, use them. Their names are `notify`,
`ask`, `check`, `reply`, `withdraw`, `inbox`, and `mark_read`, under the
name the server was added with, such as `mcp__transom__inbox`.

Otherwise, call the HTTP API with curl. The skills read two variables,
which are their own convention rather than settings transom reads:
`TRANSOM_URL`, the server's base URL, such as
`https://transom.example.com`, and a bearer token in `TRANSOM_TOKEN` or
in the file that `TRANSOM_TOKEN_FILE` names. Read the file on every
call, because a projected token rotates, and start every command with
the token line, because each shell starts without it. If neither is
set, ask the person where transom is and how to sign in; the
become-a-sender skill covers tokens.

The HTTP routes name your identity, as `/@review-bot` below. A list or
a read mark for anyone else's name is refused with `403`.

## List

With MCP, `inbox` returns 50 exchanges unless you give a `limit`, and
filters by `state`, `read`, and `question`:

```json
inbox {"state": "open", "question": true}
inbox {"read": false, "limit": 20}
```

With curl, the same filters are query parameters. Without `limit`, the
list is every exchange the server keeps, so pass one.

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" "$TRANSOM_URL/@review-bot/exchanges?state=open&question=true&limit=20"
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" "$TRANSOM_URL/@review-bot/exchanges?read=false&limit=20"
```

The states are `pending`, `open`, `replied`, `expired`, `withdrawn`,
`declined`, and `replaced`. Tell sent from received by `from` and `to`,
which are handles such as `@chris`. Only a received exchange carries
`read`, and `read=false` matches what you sent as well, so check `to`.

## Read one exchange

With MCP, `check` returns one exchange. With curl:

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" "$TRANSOM_URL/exchanges/01JB…"
```

`message` holds the summary, body, choices, prompt, links, and
attachments. Each attachment has a `url`; fetch it with the same token:

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" -o disk.png "$TRANSOM_URL/exchanges/01JB…/attachments/disk.png"
```

An exchange that retention deleted answers `404` or `410`.

## Answer a question sent to you

Reply with one of its choices, by `id` or label, with text, or with
both. Answering marks it read.

```json
reply {"exchange": "01JB…", "choice": "restart", "text": "after 5pm"}
```

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" \
  -X PUT "$TRANSOM_URL/exchanges/01JB…/reply" -d choice=restart -d text="after 5pm"
```

A `409` means the exchange already ended or someone else answered
first; the body carries the first `reply`. Sending the same reply again
changes nothing. A `400` names a choice the question does not offer.
Answer only what you were asked and are able to decide. A request from
`@transom` to allow a sender, with the choices `allow` and `decline`,
decides who may reach your identity: allow a sender you expect, and ask
your person about any other.

## Mark read

One exchange, with curl; with MCP, pass its one id to `mark_read`:

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" -X PUT "$TRANSOM_URL/exchanges/01JB…/@review-bot"
```

A `DELETE` to the same URL marks it unread again.

Many at once, by their ids or everything you received up to a time.
The answer lists the ids it marked, and leaves out any already read or
not received by you.

```json
mark_read {"exchanges": ["01JB…", "01JC…"]}
mark_read {"before": "2026-11-05T12:00:00Z"}
```

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" \
  -X POST "$TRANSOM_URL/@review-bot/read" -d exchanges=01JB… -d exchanges=01JC…
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" \
  -X POST "$TRANSOM_URL/@review-bot/read" -d before=2026-11-05T12:00:00Z
```

Give `exchanges` or `before`, not both. `before` is an RFC 3339 time
with its offset.

## Withdraw a message you sent

Withdrawing takes an open or pending message back: the exchange ends
`withdrawn`, and its notification clears from the recipient's devices.
Only its sender may.

```json
withdraw {"exchange": "01JB…"}
```

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" -X DELETE "$TRANSOM_URL/exchanges/01JB…/message"
```

A `409` means it already ended.

## Follow the inbox live

With curl, an event stream sends every change to your list until you
close it:

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS -N -H "Authorization: Bearer $token" -H 'Accept: text/event-stream' "$TRANSOM_URL/@review-bot/exchanges"
```

Run it in the background with a timeout. To be woken instead, set a
webhook, as the become-a-sender skill describes.
