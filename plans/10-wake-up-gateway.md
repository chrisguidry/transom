# 10, The wake-up gateway

Designed, with the order of the work below. Step 1, the gateway, is
built.

How a self-hosted server wakes the official app through Firebase Cloud
Messaging.

## Decided

- The project runs the gateway for the official app.
- A wake-up carries no title, body, or sender. The app wakes and syncs
  from its own server.
- The gateway holds no state.
- Phones without Google services can hold a connection open to the
  server instead.

## Proposed

### The gateway

- The gateway is a Rust crate of the workspace, `gateway/`, built into
  an image `FROM scratch` like the server's, and published as
  `ghcr.io/chrisguidry/transom-gateway`.
- It answers one request: `POST /wake` with `{"token": "<FCM
  registration token>"}`. It sends that token a data-only FCM message
  with no content at high priority, and answers `202 Accepted`. When
  FCM says the token is no longer registered, it answers `404 Not
  Found`, so the server forgets the device. When it has no Firebase
  credentials, it answers `503 Service Unavailable`.
- Any server may ask, because any server may wake the official app.
  Nothing a wake-up carries is secret, and the token is known only to
  the phone and its own server. The gateway limits each token to a few
  wake-ups a minute, in memory, so a leaked token cannot be used to
  drain a phone's battery.
- It signs in to FCM's HTTP v1 API as a Google service account, from a
  key file it reads at startup, with the OAuth 2.0 JWT bearer grant. It
  holds the access token in memory and renews it before it expires.
- It runs on the cluster that serves transom.sh, at
  `https://transom.sh/wake`, beside the site, and deploys the way the
  site does: CI publishes its manifests as
  `ghcr.io/chrisguidry/transom-gateway-deploy`, and Flux follows the
  newest tag.

### The Firebase project

- `gateway/firebase.tf` makes the Google Cloud project, turns on
  Firebase and FCM, registers the Android app `sh.transom` with the
  fingerprints of the keys that sign it, makes the gateway's service
  account with the one role that sends messages, and writes its key to
  the Secret `gateway-fcm` in the `transom` namespace of the cluster
  that runs the gateway. Its outputs are the values the app is built
  with: the project id, the app id, the API key, and the sender id.
  None of them is secret; Firebase puts them in every app.
- Applying it needs a person: a Google sign-in for Terraform, and,
  for an account that has never used Firebase, accepting its terms in
  the console. Until it is applied, the gateway answers `503` and
  phones keep their connection to the server.

### The server

- A device may wake by `{"fcm": token}`. When an exchange arrives for
  an identity, or ends, the server asks the gateway to wake each of its
  `fcm` devices, through a queue in Redis that retries like Web Push's,
  and forgets a device the gateway answers `404` for.
- The setting `wake_gateway` names the gateway, by default
  `https://transom.sh/wake`. Someone who builds the app with their own
  Firebase project runs the same gateway with their own credentials and
  points their server at it.

### The app

- The app is built with the Firebase project's values as Gradle
  properties, `transom.firebase.*`, read into `FirebaseOptions` when the
  app starts, so no `google-services.json` and no Google Services Gradle
  plugin are needed. A build without them has no FCM.
- With FCM and Google Play services, the app registers its token as an
  `{"fcm": token}` device, PUTs it again when FCM rotates it, and stops
  holding a connection open in the background. A wake-up runs one sync
  as expedited work, and the notifier shows what arrived.
- Without them, the app holds its connection as it does now.

## Built

- `gateway/` is the crate `transom-gateway`, a binary that is never
  published. `POST /wake` takes `{"token": "..."}` and nothing else, as
  JSON of at most 4 KiB, and answers:
  - `202 Accepted`, with no body, when FCM took the wake-up.
  - `400 Bad Request` for any other body or an empty token, and `413
    Payload Too Large` for a body over 4 KiB.
  - `404 Not Found` when FCM answers `404`, `UNREGISTERED`, or
    `INVALID_ARGUMENT`. The message is the same for every token, so an
    argument FCM finds invalid is the token.
  - `429 Too Many Requests`, with `Retry-After` in whole seconds, when
    the token has had its wake-ups for now.
  - `503 Service Unavailable` when the gateway has no credentials,
    cannot sign in to Google, cannot reach FCM, or FCM answers `401`,
    `429`, or `5xx`, with FCM's `Retry-After` when it gave one. After a
    `401` the gateway drops its access token and signs in again on the
    next wake-up.
  - `502 Bad Gateway` for any other refusal from FCM, such as `403`
    for a token of another Firebase project. It is not `404`, so a
    gateway with the wrong project's credentials never makes servers
    forget their devices.
  Each failure's body is `{"error": "<reason>"}`.
- The message is `{"data": {"wake": "1"}}` for the token, with
  `android.priority` `HIGH`, a TTL of 15 minutes, and the collapse key
  `wake`, so FCM holds at most one wake-up for a phone it cannot reach.
- Each token has a bucket of 5 wake-ups that gains one every 15
  seconds, so 4 a minute after the burst. The limiter keys buckets by a
  48-bit fingerprint of the token, a SHA-256 prefix, which is also how
  the request's log line names the device. No log line holds a token.
- The gateway reads the service account key at startup. A missing file
  means no credentials, and the gateway runs and answers `503`; a file
  that is not a key stops it. It signs a JWT with RS256 for the key's
  `client_email`, with the scope `firebase.messaging` and the key's
  `token_uri` as audience, trades it there with the JWT bearer grant,
  and keeps the access token until 5 minutes before it expires.
  Concurrent wake-ups wait for one trade.
- Settings are environment variables: `TRANSOM_GATEWAY_LISTEN`, by
  default `0.0.0.0:8080`; `TRANSOM_GATEWAY_CREDENTIALS`, by default
  `/var/run/secrets/fcm/service-account.json`; and
  `TRANSOM_GATEWAY_FCM_URL`, by default `https://fcm.googleapis.com`,
  which only tests change. Google's token endpoint is the key's own
  `token_uri`, so it has no setting.
- The tests run against a stand-in for Google's two endpoints, an axum
  server in the tests that verifies each JWT against the test key and
  records each message. `gateway/tests/process.rs` runs the binary.
- `gateway/Dockerfile` builds a static musl binary into an image `FROM
  scratch` that runs as UID 65532, from the root of the repository.
  `gateway/Dockerfile.dockerignore` keeps the server's source out of its
  build, and the server's image sees only the gateway's manifest.
- `gateway/deploy/` is a Deployment, a Service, and an Ingress for
  `transom.sh/wake`. The Secret `gateway-fcm` is mounted optionally, so
  the pod is ready without it; it needs a restart once the Secret
  exists. The Ingress names the site's certificate Secret, `site-tls`,
  without cert-manager's annotation, and Traefik routes the longer
  `/wake` rule ahead of the site's `/`.
- `.github/workflows/gateway.yaml` builds the image for a pull request,
  and on main publishes `ghcr.io/chrisguidry/transom-gateway` with
  build provenance and `ghcr.io/chrisguidry/transom-gateway-deploy`,
  both at a tag of the build's UTC time and short commit.

## The order of the work

1. **The gateway**: the crate, its tests against a stand-in for FCM's
   two endpoints, the image, its manifests, and CI that publishes both.
2. **The server**: `{"fcm": token}` devices and the queue to the
   gateway, tested against the gateway itself.
3. **The Firebase project**: `gateway/firebase.tf`, checked with
   `terraform validate`, and the Flux objects in the cluster's own
   repository that follow the gateway's manifests.
4. **The app**: FCM when the build has the project's values, with the
   connection as the fallback.

## Open

- Whether the app wakes by UnifiedPush as well, for phones without
  Google services that run a UnifiedPush distributor.
