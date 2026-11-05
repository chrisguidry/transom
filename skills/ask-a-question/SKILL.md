---
name: ask-a-question
description: "Ask a person a question through transom and get the answer back: offer choices, a prompt for a typed answer, or both, wait for the reply or come back for it later, and handle a question that expires, is declined, or is no longer needed. Use when you need a decision you cannot make yourself, such as approving a deploy, choosing between options, or supplying a value only the person knows; to only inform someone, use the send-a-notification skill."
---

# Ask a question

A question is a message with `choices`, a `prompt`, or both. It waits in
the recipient's inbox, on their phone and in their browser, until
someone answers it, it expires, or you withdraw it. Every field of a
notification works on a question too; the send-a-notification skill
describes `urgency`, `key`, `link`, attachments, and `ttl`.

## Calling transom

If transom's MCP tools are loaded, use them. Their names are `notify`,
`ask`, `check`, `reply`, `withdraw`, `inbox`, and `mark_read`, under the
name the server was added with, such as `mcp__transom__ask`.

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
curl -sS --fail-with-body -H "Authorization: Bearer $token" "$TRANSOM_URL/exchanges/01JB…"
```

## Write the question

Ask one thing at a time. The `summary` is the question itself, in one
line. The `body` is Markdown with what the person needs to decide: what
you found, what you plan to do, and what each choice leads to. They
may read it on a lock screen, so put the decision first.

- `choices` is a list. Each is a short label, or an object with a
  `label`, a Markdown `description`, and an `id` that the reply names.
  Up to three short choices show as buttons on the notification.
- `prompt` asks for a typed answer; its value labels the text box, and
  it may be empty. With choices too, the person can pick one, type, or
  both.
- `ttl` bounds how long the question stays open, such as `1h`. Without
  one, it stays open until someone answers or you withdraw it. Set one
  whenever the answer stops mattering after a while.
- `urgency` is `normal` unless the decision blocks something that
  matters now.

## Ask and wait

With MCP, `wait` is how many seconds `ask` waits for the exchange to
end. The tool sends progress every ten seconds to a client that asks
for it, so a long wait does not time out.

```json
ask {
  "to": "@chris",
  "summary": "Restart postgres on db-1?",
  "body": "Replication lag is **14 minutes** and rising. A restart takes about 30 s and drops open connections.",
  "choices": [{"label": "Restart now", "id": "restart"}, {"label": "Leave it", "id": "leave"}],
  "prompt": "Anything else?",
  "ttl": "1h",
  "wait": 600
}
```

With curl, `Prefer: wait=600` holds the request open for up to 600
seconds, at most an hour. It answers `200` with the exchange once it
ends, or `202` with it still open when the wait runs out.

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" \
  -H 'Content-Type: application/json' -H 'Prefer: wait=600' "$TRANSOM_URL/@chris" -d '{
    "summary": "Restart postgres on db-1?",
    "body": "Replication lag is **14 minutes** and rising. A restart takes about 30 s and drops open connections.",
    "choices": [{"label": "Restart now", "id": "restart"}, {"label": "Leave it", "id": "leave"}],
    "prompt": "Anything else?",
    "ttl": "1h"}'
```

## Read the outcome

The answer is the exchange. Its `state` says how it ended:

| `state` | Meaning | What to do |
|---|---|---|
| `open` | No answer yet. | Wait again, or come back later. |
| `pending` | You have not sent to this recipient before, and transom is asking them to allow you first. | Wait; the question opens once they allow you. |
| `replied` | `reply.choice` is the chosen choice's `id`, or its label when it has no id. `reply.text` is the typed answer. `reply.by` is who answered, which for a group is one of its members. | Act on it. |
| `expired` | The `ttl` passed with no answer. | Do not treat silence as consent. Take the safe path, or ask again. |
| `declined` | The recipient declined you as a sender. | Stop sending to them. |
| `withdrawn` | You took the question back. | Nothing. |
| `replaced` | You sent a newer message with the same `key`. | Follow the newer exchange. |

## Come back later

Keep the exchange's `id` and `url`. With MCP, `check` returns the
exchange, and waits up to `wait` seconds for it to end:

```json
check {"exchange": "01JB…", "wait": 300}
```

With curl, read the exchange, or wait for the reply alone. The reply's
URL answers `404` when the wait runs out with no reply, and when the
exchange ended without one; read the exchange then to see which.

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" "$TRANSOM_URL/exchanges/01JB…"
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" -H 'Prefer: wait=600' "$TRANSOM_URL/exchanges/01JB…/reply"
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS -N -H "Authorization: Bearer $token" -H 'Accept: text/event-stream' "$TRANSOM_URL/exchanges/01JB…/reply"
```

The last one is a stream that sends one event, `reply` or `ended`, and
closes.

To stop instead of waiting, ask without a wait and end your turn.
transom posts each outcome to your identity's webhook, with the
exchange and its `url`, so whatever wakes you can match it. The
become-a-sender skill sets the webhook.

## Take a question back

When the answer no longer matters, withdraw the question: it ends
`withdrawn`, and its notification clears from the person's devices.
Only an open or pending message can be withdrawn.

```json
withdraw {"exchange": "01JB…"}
```

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" -X DELETE "$TRANSOM_URL/exchanges/01JB…/message"
```
