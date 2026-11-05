---
name: send-a-notification
description: "Tell a person something through transom that needs no answer: a finished task, a status change, an alert, or its resolution. Covers choosing an urgency honestly, a key so a later message replaces an earlier one, links with titles, a Markdown body, images, and a time to live. Use when work should reach someone's phone or browser and nobody has to decide anything; to ask for a decision, use the ask-a-question skill."
---

# Send a notification

A notification is a message with no choices and no prompt. It reaches
the recipient's phone and browsers, and they can still reply if they
want to. A message needs a `summary`, a `body`, or both.

## Calling transom

If transom's MCP tools are loaded, use them. Their names are `notify`,
`ask`, `check`, `reply`, `withdraw`, `inbox`, and `mark_read`, under the
name the server was added with, such as `mcp__transom__notify`.

Otherwise, call the HTTP API with curl. The skills read two variables,
which are their own convention rather than settings transom reads:
`TRANSOM_URL`, the server's base URL, such as
`https://transom.example.com`, and a bearer token in `TRANSOM_TOKEN` or
in the file that `TRANSOM_TOKEN_FILE` names. Read the file on every
call, because a projected token rotates, and start every command with
the token line, because each shell starts without it. If neither is
set, ask the person where transom is and how to sign in; the
become-a-sender skill covers tokens.

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" "$TRANSOM_URL/@chris" -d "Backup finished in 4m"
```

A recipient is a name with its `@`: `@chris`, or a group such as
`@household`, whose every member receives the message. The answer is
`201` with the exchange: its `id`, its `url`, and its `state`.

A message from you to your own identity notifies no one: transom does
not push a message to its sender. If the MCP tools signed in as the
person you are writing to, send with curl and your own token.

## Choose the urgency

| Urgency | What the person gets | Use it for |
|---|---|---|
| `very-low` | Silent, in the phone's shade only; no browser notification. | A record they may look at later. |
| `low` | Silent, with an icon in the phone's status bar. | Routine results, and an alert's resolution. |
| `normal` | A notification with sound. The default. | Work finished that they are waiting on. |
| `high` | A heads-up notification with sound, kept up in the browser. | Something broken that they should look at soon. |
| `urgent` | Rings through Do Not Disturb on a phone that allows it, and rings again 5, 15, and 30 minutes after it arrives until someone reads or answers it. | Something that needs a person now, even asleep: an outage, data at risk. |

Pick the lowest urgency that is true. A person who is woken for
something that could wait stops trusting `urgent`. Most of an agent's
messages are `low` or `normal`.

## Send one

With MCP:

```json
notify {
  "to": "@chris",
  "summary": "Nightly backup finished",
  "body": "Wrote **412 GB** to the NAS in 4m 12s. No errors.",
  "urgency": "low",
  "link": [{"href": "https://grafana.example.com/d/backups", "title": "Backup dashboard"}],
  "ttl": "1d"
}
```

With curl, as JSON:

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" \
  -H 'Content-Type: application/json' "$TRANSOM_URL/@chris" -d '{
    "summary": "Nightly backup finished",
    "body": "Wrote **412 GB** to the NAS in 4m 12s. No errors.",
    "urgency": "low",
    "link": [{"href": "https://grafana.example.com/d/backups", "title": "Backup dashboard"}],
    "ttl": "1d"}'
```

The same fields work as headers on a plain-text body, which is the
message's `body`:

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" \
  -H 'Summary: Nightly backup finished' -H 'Urgency: low' \
  -H 'Link: <https://grafana.example.com/d/backups>; rel="related"; title="Backup dashboard"' \
  "$TRANSOM_URL/@chris" -d "Wrote 412 GB to the NAS in 4m 12s."
```

Use JSON when a value has non-ASCII text or a newline, or when the body
could read as a form, such as `summary=…&ttl=…`: transom reads a form
body whose names are all message fields as those fields.

- `summary` is the one line the notification shows. Say what happened,
  plainly.
- `body` is Markdown.
- `link` is a list. Each is a URL, or an object with `href`, `rel`, and
  a `title` that labels the link's button, such as "View run". Only
  `http` and `https` URLs are accepted.
- `ttl` is how long the message stays open: seconds, or a duration such
  as `90m`, `1h`, or `2d`. Once it passes, the exchange ends `expired`.
- `metadata` is a map of strings that comes back to you with the
  exchange and is never shown to the recipient.

## Replace an earlier message with a key

A `key` names what the message is about, such as an alert's
fingerprint or a job's name. A new message from you to the same
recipient with the same key replaces the earlier one while it is open:
the earlier exchange ends `replaced`, the new one names it in
`replaces`, and the recipient's devices show one notification for the
key. Send a resolution at `low` so it updates quietly.

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" \
  -H 'Urgency: high' -H 'Key: disk-full-nas' -H 'Summary: The NAS disk is 97% full' \
  "$TRANSOM_URL/@chris" -d "The backup volume will fill in about 2 hours."
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" \
  -H 'Urgency: low' -H 'Key: disk-full-nas' -H 'Summary: The NAS disk is back to 71%' \
  "$TRANSOM_URL/@chris" -d "Pruned old snapshots."
```

With MCP, pass `"key": "disk-full-nas"` to `notify` in the same way.

## Attach images

A message carries up to 10 PNG, JPEG, GIF, or WebP images, at most
5 MiB each and 20 MiB in all. The body shows one inline by naming it,
as `![the graph](disk.png)`, and the first one shows in the expanded
notification.

With curl, send a multipart form; each file part is an attachment,
named by its file name:

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" \
  -F summary="The NAS disk is filling" -F body="![the last day](disk.png)" \
  -F attachments=@disk.png "$TRANSOM_URL/@chris"
```

With MCP, or in a JSON body, each attachment is a `name` and its bytes
in base64 as `data`:

```json
notify {
  "to": "@chris",
  "summary": "The NAS disk is filling",
  "body": "![the last day](disk.png)",
  "attachments": [{"name": "disk.png", "data": "iVBORw0KGgo…"}]
}
```

## If it does not arrive

The exchange's `state` says where it stands:

- `open`: delivered.
- `pending`: you have not sent to this recipient before, and transom is
  asking them to allow you. The become-a-sender skill covers consent.
- `declined`: the recipient declined you as a sender.

A `404` means no one holds that name. A `400` says which field is
wrong. A `401` means the token is missing, expired, or from an issuer
transom does not trust.
