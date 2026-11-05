# 13, The HTTP API

Closed 2026-11-05. Built.

transom's whole HTTP API in one place, mostly as examples. Plan 02 has
the reasons behind identities and consent, plan 04 behind exchanges,
and plan 06 behind webhooks. Every request carries
`Authorization: Bearer <token>` from a trusted issuer, as plan 03
describes, or a session's token that starts with `transom_`, as the
app holds after it signs in.

## Identities

```sh
curl -X PUT   transom/@review-bot                        # claim a name: 201, or 403 if it's someone else's
curl          transom/@chris                             # describe: {"name": "chris", "kind": "individual"}
curl -X PATCH transom/@chris -d '{"name": "guid.foo"}'   # rename; /@chris now answers 308 → /@guid.foo
curl -X PATCH transom/@review-bot -d '{"webhook": "https://wicket/transom/review-bot"}'
curl -X PUT   transom/@household                         # from an existing individual: creates a group
curl -X PUT    transom/@household/members/@spouse        # invite: @transom asks @spouse to consent
curl -X DELETE transom/@household/members/@chris         # leave or remove
```

An identity's display name and picture are what clients show in place
of its handle. Whoever may rename an identity sets them. A display name
is at most 64 characters, and an empty one removes it. A picture is a
square PNG, JPEG, or WebP of at most 256 KiB and 1024 pixels a side,
typed by its bytes rather than its `Content-Type`; anything else is
`415`, `413`, or `422`. Anyone signed in may fetch a picture.

```sh
curl -X PATCH transom/@grafana -d '{"display_name": "Grafana"}'
curl -X PUT --data-binary @grafana.png transom/@grafana/picture   # 200 with the identity
curl -X DELETE transom/@grafana/picture                          # 204
curl transom/@grafana                     # {"name": "grafana", "kind": "individual",
                                          #  "display_name": "Grafana", "picture": "/@grafana/picture?v=3f2a…"}
curl transom/@grafana/picture             # the image, with ETag: "3f2a…"; 304 for a matching If-None-Match
```

The picture's URL carries its version, a hash of its bytes, so a new
picture has a new URL; it is absolute when the settings give a
`public_url`, and `null` when there is no picture. An exchange names
each party the same way beside its handle: `from_name` and
`from_picture`, `to_name` and `to_picture`, and the reply's `by_name`
and `by_picture`, each `null` when unset.

## Sending

A POST to a name answers `201 Created`, with `Location: /exchanges/{id}`. The
exchange's `url` is absolute when the settings give a `public_url`, so a
webhook's receiver can follow it.

```sh
curl -d "Backup finished in 4m" transom/@chris
curl -d @context.md \
  -H 'Summary: Restart postgres?' \
  -H 'Choices: "Restart", "Leave it"' \
  -H 'Prompt: Why not?' \
  -H 'Urgency: high' \
  -H 'TTL: 3600' \
  -H 'Link: <https://grafana/d/disk>; rel="related"; title="Disk usage"' \
  -H 'Metadata: session="abc"' \
  -H 'Prefer: wait=600' \
  transom/@chris                                         # 200 with the outcome, or 202 if the wait runs out
curl -H 'Content-Type: application/json' transom/@chris -d '{
  "summary": "Restart postgres?", "body": "…",
  "choices": ["Leave it", {"label": "Restart", "id": "restart", "description": "…"}],
  "prompt": "", "urgency": "high", "ttl": "1h", "metadata": {"session": "abc"}}'
curl -H 'Urgency: urgent' -H 'Key: disk-full' -d "Disk is full" transom/@chris   # shown again at 5, 15, and 30 minutes until read
curl -H 'Urgency: low' -H 'Key: disk-full' -d "Disk is fine again" transom/@chris # replaces it: {"replaces": "01JB…"}
curl -F summary="Disk is filling" -F body="![the graph](disk.png)" -F attachments=@disk.png transom/@chris
curl -H 'Content-Type: application/json' transom/@chris -d '{
  "summary": "Disk is filling", "attachments": [{"name": "disk.png", "data": "<base64>"}]}'
curl transom/exchanges/01JB…/attachments/disk.png   # the image, to whoever may see the exchange
```

## The exchange and its legs

```sh
curl transom/exchanges/01JB…                  # {"state": "open", "message": {…}, "reply": null}
curl transom/exchanges/01JB…/message
curl -X PUT transom/exchanges/01JB…/reply -d choice=restart -d text="after 5pm"   # 200, or 409 if someone else replied first
curl -X PUT transom/exchanges/01JB…/reply -d "don't do this at all ya dingus"     # plain text is the reply's text
curl -X DELETE transom/exchanges/01JB…/message   # withdraw: the exchange becomes withdrawn
curl transom/exchanges/01JB…/message             # 410 Gone, its contents still in the body
```

## Waiting for the outcome

```sh
curl -H 'Prefer: wait=600' transom/exchanges/01JB…/reply              # the reply, or 404 when the wait runs out
curl -H 'Accept: text/event-stream' transom/exchanges/01JB…/reply     # one event, then the stream closes
curl -H 'Accept: text/event-stream' transom/exchanges/01JB…           # where it stands, then every change until the end
curl -H 'Accept: text/event-stream' transom/exchanges/01JB…?changes   # only the changes
```

## An identity's exchanges

```sh
curl transom/@chris/exchanges                                  # sent and received: every one the server keeps
curl transom/@chris/exchanges?limit=20                         # the newest 20
curl transom/@chris/exchanges?state=open                       # what's waiting
curl -H 'Accept: text/event-stream' transom/@chris/exchanges   # the live feed
```

## Read state, private to each recipient

```sh
curl -X PUT    transom/exchanges/01JB…/@chris     # read: devices clear its notification
curl -X DELETE transom/exchanges/01JB…/@chris     # unread again
curl transom/@chris/exchanges?read=false          # what's new
```

A POST to `/@{name}/read` marks many exchanges read at once, for the
caller only: the exchanges it lists, or with `before`, every exchange
the caller received, itself or through a group, up to that time. The
body is JSON, YAML, or a form, in which each id is one more `exchanges`
field. The answer lists the ids it marked; it leaves out an exchange
that was already read, and one the caller did not receive. Each mark
reaches the caller's live feeds as a single read does. Each of the
caller's browsers gets one push, `{"event": "read", "ids": [...]}`, with
up to 100 ids, and as many pushes as more ids need. Each of the
caller's phones that wakes by FCM gets one wake-up, after which it
syncs and finds the exchanges read.

```sh
curl -X POST transom/@chris/read -d exchanges=01JB… -d exchanges=01JC…   # {"read": ["01JB…", "01JC…"]}
curl -X POST transom/@chris/read -d before=2026-11-05T12:00:00Z          # everything received up to then
curl -X POST transom/@chris/read -H 'Content-Type: application/json' -d '{"exchanges": ["01JB…"]}'
```

When a person dismisses a notification in a browser, the browser marks
its exchange read, as swiping one away does on the phone: each
arrival's push names the identity it was pushed to as `reader`. An exchange that
retention deletes while its notification may still show, because it is
open and unread, closes that notification with a push of
`{"event": "gone", "id": "01JB…"}`. Phones are not woken for a
deletion; each drops the notification at its next sync.

## Devices

A browser wakes by Web Push, with the subscription its
`PushSubscription.toJSON()` writes. The listing carries the key a
browser subscribes with. A phone that holds the live feed open wakes
by `{"connection": true}`, and nothing is pushed to it. A phone running
the official app with FCM wakes by `{"fcm": "<installation ID>"}`,
and the server asks the wake-up gateway to wake it for each arrival,
end, and read, as plan 10 says. A device PUT
with the app's session token ends that session when it is deleted.

```sh
curl -X PUT transom/@chris/devices/7c1f… -d '{"name": "Pixel 9", "wake": {"connection": true}}'
curl -X PUT transom/@chris/devices/7c1f… -d '{"name": "Pixel 9", "wake": {"fcm": "<installation ID>"}}'
curl -X PUT transom/@chris/devices/b4e0… -d '{"name": "Firefox", "wake": {"web_push": {"endpoint": "https://…", "keys": {"p256dh": "…", "auth": "…"}}}}'
curl        transom/@chris/devices                 # your devices, and {"key": "<VAPID public key>"}
curl -X DELETE transom/@chris/devices/7c1f…        # sign out, or forget a lost phone
```

## Webhooks

```sh
curl -X PATCH transom/@review-bot -d '{"webhook": "https://wicket/transom/review-bot"}'
curl transom/.well-known/jwks.json                 # the key webhooks are signed with
```

## Signing in a browser

Any of these URLs, asked for with `Accept: text/html`, is its page.

```sh
open https://transom/login                          # the issuers to sign in with
open https://transom/login/authentik?next=/         # to the issuer, and back to /login/callback
curl -X POST -H 'HX-Request: true' transom/logout   # signed in by cookie: ends the session
```

## Signing in the app

The app makes a PKCE verifier, and opens `/login` in a Custom Tab with
its S256 challenge and the callback to send the code to, one of the
server's `app_callbacks`. The person signs in as in a browser, and the
issuer sends the Custom Tab back to `/login/callback`, which sends it on
to the callback with a code that works once, for a minute.

```sh
open 'https://transom/login?app=<challenge>&callback=https://transom.sh/app/callback'
# … the Custom Tab lands at https://transom.sh/app/callback?code=<code>
curl -H 'Content-Type: application/json' transom/login/app \
  -d '{"code": "<code>", "verifier": "<verifier>"}'   # {"token": "transom_…"}, or 400
curl -H 'Authorization: Bearer transom_…' transom/@chris/exchanges
curl -X POST -H 'Authorization: Bearer transom_…' transom/logout   # 204: ends the session
```

## States

`pending` → `open` → `replied`, `expired`, `withdrawn`, `declined`, or
`replaced`. An exchange is `replaced` when the same sender sends the
same recipient a message with the same `key` while it is open. The new
exchange names it in `replaces`, and an `urgent` one counts the times it
was shown again in `repeats`.
A stranger's first message is `pending` while `@transom` asks the
recipient to Allow or Decline. A webhook, signed with RFC 9421, fires
for every outcome and every new exchange.

Once the server's `retention`, 2 days by default, has passed since an
exchange ended, or since a notification arrived, the server deletes it:
`GET /exchanges/{id}` answers `404`, the list leaves it out, and the
live feed sends `gone` with its id. An open question is kept until it
ends.
