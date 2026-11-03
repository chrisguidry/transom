# 04, Messages and questions

Not built. Stub.

What a message carries, how a question is answered, and a question's
lifecycle.

## Decided

- The central object is a **message**. A message with choices or a reply
  box is a question, and one with neither is a notification.
- A question is answered with a choice, typed text, or both.
- A question has a one-line summary, an optional preview for the
  notification, a Markdown body with its context, and for each choice a
  label and an optional Markdown description.
- A question is open until it is answered, expires, or is withdrawn, and
  the first answer wins.
- A deadline is optional and set by the asker. A question without one
  stays open until it is answered or withdrawn. At its deadline, a
  question expires: its notification clears, the inbox marks it
  expired, and the asker gets the outcome `expired`. A notification
  with a deadline is removed from devices when it passes.
- A question is always answerable from its notification. The person
  decides whether they have read enough.
- The asker can attach its own data, which comes back with the answer.
- A one-way notification is the same object without choices or a reply
  box: a summary, an optional short version, and a Markdown body.
- A message can carry a link, opened when its notification is tapped,
  and an importance level. On Android each level is a notification
  channel, so each person decides what each level does.
- Images are sent with the message as attachments, and the body places
  them inline. transom serves them behind the same sign-in as the
  message. Video comes later.

## Sending

A message is sent with a POST to its recipient's `/@{name}`.

The body of the request can be JSON, YAML, an HTML form
(`application/x-www-form-urlencoded` or `multipart/form-data`), or plain
text or Markdown (`text/plain` or `text/markdown`). A plain text or
Markdown request body is the message's body, which makes the simplest
send one curl command.

The fields are the same in every form. Each one except `body` and
`attachments` can also be a query parameter or an HTTP header of the same
name. None of the names is a registered HTTP header, so no header needs
a prefix.

| Field | Holds |
| --- | --- |
| `summary` | The one line a notification shows. |
| `preview` | The few sentences an expanded notification shows. |
| `body` | The Markdown context: the request body, or a `body` field. Never a header. |
| `choices` | The choices, each a label and an optional Markdown description. A header or query parameter carries labels only: `Choices: Deploy, Hold`. |
| `reply` | Allows a typed reply. Its value is the reply box's placeholder. |
| `deadline` | When the message expires. |
| `url` | Opened when the notification is tapped. |
| `importance` | How loudly the phone announces it. |
| `callback` | Where the outcome is POSTed. |
| `metadata` | The asker's own data, returned with the answer. As a header: `Metadata: session=abc, run=7`. |
| `attachments` | Images, in a `multipart/form-data` body. |

- An answer is a choice, typed text, or both, and needs at least one of
  what the question offers.
- A field given in two places with different values gets
  `400 Bad Request`.
- How long the request waits for an answer is not a field of the message.
  It is the standard `Prefer: wait=3600` header (RFC 7240), or a `wait`
  query parameter such as `?wait=1h`.
