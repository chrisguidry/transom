# 13, The HTTP API

Built, except devices (plan 09) and attachments.

transom's whole HTTP API in one place, mostly as examples. Plan 02 has
the reasons behind identities and consent, plan 04 behind exchanges,
and plan 06 behind webhooks. Every request carries
`Authorization: Bearer <token>` from a trusted issuer, as plan 03
describes.

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

## Sending

A POST to a name answers `201 Created`, with `Location: /exchanges/{id}`.

```sh
curl -d "Backup finished in 4m" transom/@chris
curl -d @context.md \
  -H 'Summary: Restart postgres?' \
  -H 'Choices: "Restart", "Leave it"' \
  -H 'Prompt: Why not?' \
  -H 'Urgency: high' \
  -H 'TTL: 3600' \
  -H 'Link: <https://grafana/d/disk>; rel="related"' \
  -H 'Metadata: session="abc"' \
  -H 'Prefer: wait=600' \
  transom/@chris                                         # 200 with the outcome, or 202 if the wait runs out
curl -H 'Content-Type: application/json' transom/@chris -d '{
  "summary": "Restart postgres?", "body": "…",
  "choices": ["Leave it", {"label": "Restart", "id": "restart", "description": "…"}],
  "prompt": "", "urgency": "high", "ttl": "1h", "metadata": {"session": "abc"}}'
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
curl -H 'Accept: text/event-stream' transom/exchanges/01JB…           # every state change until the end
```

## An identity's exchanges

```sh
curl transom/@chris/exchanges                                  # sent and received
curl transom/@chris/exchanges?state=open                       # what's waiting
curl -H 'Accept: text/event-stream' transom/@chris/exchanges   # the live feed
```

## Read state, private to each recipient

```sh
curl -X PUT    transom/exchanges/01JB…/@chris     # read: devices clear its notification
curl -X DELETE transom/exchanges/01JB…/@chris     # unread again
curl transom/@chris/exchanges?read=false          # what's new
```

## Devices

```sh
curl -X PUT transom/@chris/devices/7c1f… -d '{"name": "Pixel 9", "wake": {"fcm": "<registration token>"}}'
curl        transom/@chris/devices                 # your devices
curl -X DELETE transom/@chris/devices/7c1f…        # sign out, or forget a lost phone
```

## Webhooks

```sh
curl -X PATCH transom/@review-bot -d '{"webhook": "https://wicket/transom/review-bot"}'
curl transom/.well-known/jwks.json                 # the key webhooks are signed with
```

## States

`pending` → `open` → `replied`, `expired`, `withdrawn`, or `declined`.
A stranger's first message is `pending` while `@transom` asks the
recipient to Allow or Decline. A webhook, signed with RFC 9421, fires
for every outcome and every new exchange.
