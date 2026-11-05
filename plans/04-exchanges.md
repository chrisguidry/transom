# 04, Exchanges

Built.

What an exchange carries, how it is sent and replied to, and how the
asker learns the outcome.

## Decided

- The central object is an **exchange**, at `/exchanges/{id}`. It has
  two legs: the **message**, which the sender sends, and the **reply**,
  if there is one.
- Any message can be replied to with free text, even a one-way
  notification, so replying to an alert can start or feed a triage
  session. A reply goes to the sender's webhook like any other event for
  that identity.
- A message with choices or a `prompt` is a question: it shows as open
  in the inbox until it is replied to, and the sender can wait on it. A
  message with neither is a notification.
- A reply is a choice, text, or both. Text is always allowed.
- A message has a short summary, a Markdown body with its context, and
  for each choice a label and an optional Markdown description. A
  notification shows the summary, and expanded, the start of the body.
- A question is open until it is replied to, expires, or is withdrawn,
  and the first reply wins.
- A deadline is optional and set by the sender. A question without one
  stays open until it is replied to or withdrawn. At its deadline, a
  question expires: its notification clears, the inbox marks it
  expired, and the sender gets the outcome `expired`. A notification
  with a deadline is removed from devices when it passes.
- A question is always answerable from its notification. The person
  decides whether they have read enough.
- Images are sent with the message as attachments, and the body places
  them inline. transom serves them behind the same sign-in as the
  message. Video comes later.
- transom deletes an exchange once the `retention` setting, 2 days by
  default, has passed since it ended: replied, expired, withdrawn, or
  declined. A notification is deleted once retention has passed since it
  arrived, even if it was replied to since. An open question is never
  deleted for its age; its own `ttl` bounds it. A deleted exchange is
  gone completely, and its URL answers `404` like any unknown id.
- Since retention bounds what the server keeps, the list has no paging:
  it is every exchange the server keeps, unless the caller asks for
  fewer.

## Sending

A message is sent with a POST to its recipient's `/@{name}`.

The body of the request can be JSON, YAML, an HTML form
(`application/x-www-form-urlencoded` or `multipart/form-data`), or plain
text or Markdown (`text/plain` or `text/markdown`). A plain text or
Markdown request body is the message's body, which makes the simplest
send one curl command.

The fields are the same in every form. Each one except `body` and
`attachments` can also be a query parameter or an HTTP header of the same
name. Where a registered HTTP header means the same thing, the field
takes its name and its syntax: `link` is Web Linking's `Link` (RFC 8288),
and `urgency` and `ttl` are the Web Push protocol's `Urgency` and `TTL`
(RFC 8030). The others are names of transom's own, none of them a
registered header. Header values with more than one part are Structured
Fields (RFC 9651).

| Field | Holds |
| --- | --- |
| `summary` | The sender's short version, which a notification shows. |
| `body` | The Markdown context: the request body, or a `body` field. Never a header. |
| `choices` | The choices. Each is a label, or an object with a `label`, an optional Markdown `description`, and an optional `id`. A header carries labels only: `Choices: "Deploy now", "Hold"`. |
| `prompt` | Makes the message a question that asks for text. Its value is the text box's prompt, and it may be empty. |
| `ttl` | How long the message stays open before it expires: seconds, as RFC 8030 defines, or a duration such as `1h` or `2d`. |
| `link` | Links that go with the message, with registered relation types, as in `Link: <https://grafana.example.com/d/disk>; rel="related"`. Tapping the notification opens the first. |
| `urgency` | How urgently to deliver it: `very-low`, `low`, `normal`, or `high`. On Android each urgency is a notification channel, so each person decides what each one does. |
| `metadata` | The sender's own data, returned with the outcome and never shown to the recipient. As a header: `Metadata: session="abc", run=7`. |
| `attachments` | Images, in a `multipart/form-data` body. |

- A field given in two places with different values gets
  `400 Bad Request`.
- How long the request waits for a reply is not a field of the message.
  It is the standard `Prefer: wait=3600` header (RFC 7240), or a `wait`
  query parameter such as `?wait=1h`.

## The exchange and its legs

- `GET /exchanges/{id}` is the whole exchange: its state, its `message`,
  and its `reply`, which is `null` until there is one.
- `GET /exchanges/{id}/message` is the sender's leg, which never changes.
- `PUT /exchanges/{id}/reply` replies, with `choice` and `text`, in the
  same body forms as a send; a plain text or Markdown body is the text.
  Only the exchange's recipients may reply. PUT is idempotent: repeating
  the same reply from the same person gets `200`, so a phone can safely
  retry one it queued offline, and a different reply after the first
  gets `409 Conflict` with the reply that won. A client that must create
  the reply, not repeat it, sends `If-None-Match: *` and gets
  `412 Precondition Failed` if a reply exists.
- `GET /exchanges/{id}/reply` is the reply, once there is one.
- `DELETE /exchanges/{id}/message` withdraws the message: the sender takes
  it back while the exchange is open. The exchange becomes `withdrawn`,
  devices clear its notification, and a GET on the message answers
  `410 Gone` with its contents still in the body. Only the sender may
  withdraw, and a withdrawal after a reply gets `409 Conflict`.

## Read state

Each recipient has a private read state on each exchange, at
`/exchanges/{id}/@{name}`. A PUT marks it read, and a DELETE marks it
unread again. Opening the exchange, tapping or swiping away its
notification, and replying all mark it read, and devices clear its
notification when it is. Only that recipient can see it: it is never part
of `GET /exchanges/{id}`, so no sender or other group member learns it.
The recipient's own list can filter on it, as in
`/@{name}/exchanges?read=false`.

## Waiting for the outcome

A send answers `201 Created`, with the exchange's URL in `Location` and
its state. The sender learns the outcome whichever way suits it:

- **Waiting in the send.** With `Prefer: wait`, the POST stays open. An
  outcome in time comes back in the response; a wait that runs out
  answers `202 Accepted` with the exchange's URL, as RFC 7240 says.
- **Waiting for the reply.** `GET /exchanges/{id}/reply` with
  `Prefer: wait` answers as soon as the reply exists, or `404` when the
  wait runs out, so a script can loop on it.
- **Streaming.** `GET /exchanges/{id}` with `Accept: text/event-stream`
  streams each change of the exchange's state. The same on `/reply`
  streams just the reply. Either stream sends the outcome and closes
  when the exchange ends, with a reply or without one, so nothing waits
  on a reply that will never come.
- **Being called.** The sender's webhook receives the outcome, as plan
  06 describes.

An exchange from a sender the recipient has not accepted yet starts as
`pending`, and ends as `declined` if the recipient declines it.

An outcome is the exchange's state, `replied`, `expired`, `withdrawn`,
or `declined`, with the reply when there is one: its `choice` and `text`,
who replied, and when. A reply names a choice by its `id`, or by its
label when it has none. The sender's `metadata` comes back with it.

## Built

- `server/src/exchanges/` holds the exchanges: reading a send in every
  form (`reading.rs`, `fields.rs`, `links.rs`), the legs (`legs.rs`),
  waiting and streams (`waiting.rs`), the list (`listing.rs`), expiry
  (`expiry.rs`), retention (`retention.rs`), and consent (`consent.rs`).
- `curl -d text` sends a form content type with any text, so a form body
  is read as fields only when every name in it is a field. Otherwise the
  whole body is the message's text.
- A message needs a summary or a body. Links must be `http` or `https`
  URLs, and choices need labels and distinct ids.
- A header that is not valid UTF-8 keeps what can be read of it.
- An exchange's JSON says whether it is a question, and the list filters
  on that with `?question=`, alongside `?state=`, `?read=`, and `?limit=`.
  Without `?limit=`, the list is every exchange the server keeps. The MCP
  server's `inbox` tool returns 50 unless the agent asks for more, since
  an agent reads the whole answer into its context.
- A duration, in a `ttl`, a wait, or the `retention` setting, counts a
  day as 24 hours and a week as 7 days. A month or a year has no fixed
  length, so neither is a duration.
- A wait is held for an hour at most.
- Expiry runs once a second, in a pass that starts at once, so exchanges
  that came due while no server ran expire at startup. An expiry entry
  for an exchange that ended another way, or is gone, is dropped.
- Retention, in `retention.rs`, runs in the same pass. When an exchange
  ends, or a notification arrives, its id goes into the sorted set
  `deletions`, scored by when it is due, unless it is there already; so
  a notification replied to later keeps the time it arrived by. Each pass
  deletes up to 500 that are due, each in one Lua script that first takes
  the id out of `deletions`, so when two servers share a Redis only one
  of them deletes and announces each exchange. The script deletes the
  exchange's hash and `attachments:{id}`, and takes the id out of
  `sent:` and `received:`, `expiries`, a hold for consent, and the read
  state of the recipient, its members, and every reader the hash
  records, which covers a member who has since left the group.
- A deleted exchange is announced on its channel and its sender's and
  recipient's, and the live feed sends `gone` with its id, as it does
  for a deleted group's exchanges, so the web inbox and the app drop it
  at once. A retention deletion leaves no tombstone: a deleted group's
  exchanges answer `410 Gone`, but an exchange deleted for its age
  answers `404`, and a group's tombstone that was due is deleted with
  it.
- Exchanges written before retention have no entry in `deletions`. In
  each server's first pass that reaches Redis, a backfill scans every
  `exchange:` hash and schedules what has ended or arrived: a
  notification from its creation, a reply from its time, and an expiry
  from its deadline. A withdrawal or a decline records no time, so one
  of those is scheduled from the backfill itself, and is kept a whole
  retention from then. Scheduling leaves an entry that exists alone, so
  the backfill changes nothing when it runs again, and it also schedules
  an exchange whose server stopped between its change and its
  announcement.
- A read mark is a Lua script that does nothing once the exchange is
  gone, so a mark that races a deletion leaves no read state behind.
- Waiting requests and event streams each hold a Redis Pub/Sub
  subscription to the exchange's channel, or to an identity's and its
  groups' channels, subscribing before they first look so no change is
  missed between the two.
- Attachments are PNG, JPEG, GIF, or WebP images, known by their first
  bytes rather than the type the sender gives; SVG is left out because
  it can carry script. A message carries at most 10, each at most 5 MiB
  and 20 MiB in all. They arrive as the files of a multipart body, or in
  JSON, YAML, and MCP as `attachments`, a list of `{name, data}` with the
  data in base64. Each name is made plain, keeping letters, digits, `.`,
  `-`, and `_`, and must be the message's only one of that name.
- The message lists each attachment's name, type, size, and URL,
  `/exchanges/{id}/attachments/{name}`, which serves it to whoever may
  see the exchange, sandboxed by its Content Security Policy. Redis keeps
  the bytes in `attachments:{id}`, deleted with the exchange. A message
  with no attachments lists `"attachments": []`.
- A body places an attachment by naming it as an image,
  `![the graph](graph.png)`, and the page shows it there; any other image
  in a body is still a link. The page shows the attachments the body does
  not place after it.
- A change to an identity's read state is published on that identity's
  own channel only, so its live feeds send the exchange again with its
  new `read`, and no sender or other group member hears it. A mark that
  changes nothing announces nothing. A reply's read announces nothing
  more than the reply's end, which already reaches the replier's feeds
  and devices.
- The list and the live feed give an exchange `read` only when the viewer
  is one of its recipients, so a sender's list and feed carry no read
  state.
