---
name: become-a-sender
description: "Set up an agent's identity in transom: sign in with a token from an issuer transom trusts, such as a Kubernetes projected ServiceAccount token or an OAuth access token, claim a name, set a display name, a picture, and a webhook, and get a recipient's consent for the first message. Use before an agent or a service sends its first message, when a send answers 401 or 409, when its messages stay pending, or when it should show a name and picture instead of a handle."
---

# Become a sender

transom holds no passwords or API keys. Every request carries a bearer
token from an issuer that the server's settings trust, and the token's
issuer and subject map to one identity, an individual with a name such
as `@review-bot`. The person sees that name, or its display name and
picture, on every message.

## Calling transom

The MCP tools sign in with the token of whoever connected the server,
and have no tools for setting up an identity. Set one up with curl.

The skills read two variables, which are their own convention rather
than settings transom reads: `TRANSOM_URL`, the server's base URL, such
as `https://transom.example.com`, and a bearer token in `TRANSOM_TOKEN`
or in the file that `TRANSOM_TOKEN_FILE` names. Read the file on every
call, because a projected token rotates, and start every command with
the token line, because each shell starts without it.

## Get a token

An issuer is trusted by its URL and one audience, which the server's
settings name.

- **In Kubernetes**, a projected ServiceAccount token for transom's
  audience, often `transom`. The kubelet rotates the file, so read it on
  each request.

  ```yaml
  volumes:
    - name: transom
      projected:
        sources:
          - serviceAccountToken:
              audience: transom
              expirationSeconds: 3600
              path: token
  ```

  Mount it at `/var/run/secrets/transom` and set
  `TRANSOM_TOKEN_FILE=/var/run/secrets/transom/token`. The server must
  list the cluster as an issuer.
- **Anywhere else**, an OAuth or OpenID Connect access token from an
  issuer that transom trusts, issued for its audience, in
  `TRANSOM_TOKEN`. How to get one depends on the issuer, such as a
  client credentials grant for a service account.

A `401` means the token is missing, expired, for another audience, or
from an untrusted issuer. A `503` means transom could not reach the
issuer to check it.

## Claim a name

A token whose issuer gives a name claim, such as `preferred_username`,
gets an individual under that name on its first request. A Kubernetes
token carries no name, so its first request answers `409`: claim one.

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" -X PUT "$TRANSOM_URL/@review-bot"
```

| Answer | Meaning |
|---|---|
| `201` | The name is yours now. |
| `200` | It was already yours. |
| `403` | Someone else holds it. Choose another. |
| `308` | It is someone's former name; `Location` names that identity's current one. |

A name is lowercase letters, digits, and hyphens, in labels joined by
dots, like a hostname. Claim it once. A `PUT` of another free name from
a token that already has an identity creates a group with you as its
first member, not a second name for you; rename with a `PATCH`
instead:

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" \
  -X PATCH -H 'Content-Type: application/json' "$TRANSOM_URL/@review-bot" -d '{"name": "reviewer"}'
```

## Set a display name and a picture

Clients show the display name and picture in place of the handle. A
display name is at most 64 characters, and an empty one removes it.

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" \
  -X PATCH -H 'Content-Type: application/json' "$TRANSOM_URL/@review-bot" -d '{"display_name": "Review Bot"}'
```

A picture is a square PNG, JPEG, or WebP of at most 256 KiB and 1024
pixels a side. transom answers `422` for one that is not square or is
larger than 1024 pixels, `413` for one over 256 KiB, and `415` for
another format.

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" \
  -X PUT --data-binary @review-bot.png "$TRANSOM_URL/@review-bot/picture"
```

A `DELETE` to the same URL removes it. `GET /@review-bot` shows the
identity as it stands.

## Set a webhook

A webhook lets an agent stop instead of waiting. transom POSTs
`{"event": "received", "exchange": {…}}` when a message reaches the
identity, and `{"event": "ended", "exchange": {…}}` when one it sent
ends, each signed with RFC 9421 by the key at
`/.well-known/jwks.json`.

```sh
token=${TRANSOM_TOKEN:-$(cat "$TRANSOM_TOKEN_FILE")}
curl -sS --fail-with-body -H "Authorization: Bearer $token" \
  -X PATCH -H 'Content-Type: application/json' "$TRANSOM_URL/@review-bot" \
  -d '{"webhook": "https://hooks.example.com/transom/review-bot"}'
```

An empty `webhook` removes it.

## Get consent

A recipient allows each new sender once. Your first message to someone
is held as `pending`, and `@transom` asks them to Allow or Decline you,
quoting that message. If they allow you, every held message opens, and
later ones arrive at once. If they decline, the held messages end
`declined`. Members of a group may always send to it.

Make the first message one the person will want to allow: a clear
summary that says who you are and why you are writing. Do not send
again and again while it is pending; each new message is held too, and
the person is asked only once.
