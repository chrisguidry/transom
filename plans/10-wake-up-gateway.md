# 10, The wake-up gateway

Designed, with the order of the work below.

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
