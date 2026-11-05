# 10, The wake-up gateway

All four steps below are built, and the Firebase project is applied.
Stage 3 is done when a phone that has sat idle gets a question within
seconds, which has not been measured.

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
- Step 3, the Firebase project, in `gateway/firebase.tf`, checked with
  `terraform validate`, and applied by a person as `gateway/README.md`
  says. The project `transom-wake` has no billing
  account and no organization. It turns on Service Usage, Cloud
  Resource Manager, Firebase, FCM, FCM Registrations, Firebase
  Installations, and IAM. The Android app `sh.transom` takes its
  fingerprints from the variable `android_signing_sha256`, the debug
  key's today. The service account `gateway` holds
  `roles/firebasecloudmessaging.admin`, the narrowest predefined role
  with `cloudmessaging.messages.create`, and its key goes to the Secret
  `gateway-fcm` through the `public-sites` context of
  `~/.kube/config`.
- The outputs `project_id`, `application_id`, `api_key`, and
  `sender_id` are the app's `transom.firebase.*` Gradle properties, and
  `gradle_properties` prints all four as lines of a gradle.properties
  file. The API key comes from the google-services.json that Firebase
  writes for the app.
- Step 2, the server. A device PUTs `{"wake": {"fcm": "<registration
  token>"}}`: a token of 1 to 1024 letters, digits, `-`, `_`, and `:`,
  since FCM's are URL-safe base64 with a `:` between two parts. The
  devices list shows it as it does the others, and the devices page says
  it wakes through Firebase Cloud Messaging.
- Wherever the server pushes by Web Push, it also queues a wake-up for
  each `fcm` device of the same identities: an exchange's arrival and
  its end, for the recipient and its members except the sender, and a
  read, for the reader alone. The queue is `wakes` in Redis, a sorted
  set of ids by when each is due, beside `wake:{id}`, a hash of the
  identity, the device, the attempts, and when it was first queued. It
  holds no token: each attempt reads the device's current token, so a
  phone that registers a new one is woken by it.
- The once-a-second pass drains it, after the pushes. Each wake-up is
  one `POST` to the setting `wake_gateway` with `{"token": "..."}`, and
  a phone with several wake-ups due in one pass is woken once.
  - `2xx`: done.
  - `404` with the gateway's `{"error": ...}` body: the server forgets
    the device. A `404` without that body, as from a `wake_gateway` that
    names the wrong URL, is a refusal, so a mistaken setting never makes
    the server forget phones.
  - `429`, `503`, any other `5xx` but `502`, or no answer: tried again,
    after the `Retry-After` the gateway gave in whole seconds, and
    otherwise after a backoff that doubles from a second up to five
    minutes, until an hour after the wake-up was queued.
  - `400`, `502`, and anything else: logged as an error and dropped.
  The server's log lines name the phone by the gateway's fingerprint,
  the first 6 octets of the token's SHA-256 in hex, so the two logs
  agree, and no log line holds a token.
- The setting `wake_gateway`, `TRANSOM_WAKE_GATEWAY` in the
  environment, is by default `https://transom.sh/wake`.
- The server's tests run the gateway's binary, which Cargo builds beside
  the server's test binary when it builds the workspace's tests, against
  a stand-in for Google's two endpoints that answers by the token, so
  the server is tested against the gateway's own answers: `202`, `404`,
  `429` from its limit, `503` with `Retry-After` for FCM's `429`, and
  `502` for FCM's `403`.
- The public-sites repository follows
  `ghcr.io/chrisguidry/transom-gateway-deploy` in `transom/gateway.yaml`
  with an OCIRepository and a Kustomization into `transom`, and its
  ImagePolicy in `transom/images.yaml` moves the tag.
### The app

- The Gradle properties `transom.firebase.projectId`,
  `transom.firebase.applicationId`, `transom.firebase.apiKey`, and
  `transom.firebase.senderId` become BuildConfig fields, empty when
  absent. CI passes them from the repository's variables
  `TRANSOM_FIREBASE_*`. The manifest removes Firebase's
  `FirebaseInitProvider`, so a build without the values never starts
  Firebase. The app depends on `firebase-messaging` 26.0.1, from
  Google's Maven, and on `play-services-base` 18.9.0, the version
  `firebase-messaging` asks for, for `GoogleApiAvailability`.
  `play-services-base` asks for Fragment 1.1.0, whose activity results
  lint rejects, so the app names Fragment 1.9.1.
- `Firebase.push()` in the `wake` package decides once per process: with
  all four values and Google Play services available, it starts the
  default `FirebaseApp` from `FirebaseOptions` and returns a `Push`.
  Otherwise there is no FCM.
- The token is the Firebase installation ID. `firebase-messaging` 26
  deprecates registration tokens, `getToken()`, and `onNewToken()` for
  registering by installation ID, which the manifest turns on with
  `firebase_messaging_installation_id_enabled`. `Push.token()` calls
  `FirebaseMessaging.register()` and returns the ID once FCM has
  registered it, and `WakeService.onRegistered()` hands the app each ID
  FCM registers after that. The gateway sends the ID as FCM's
  `message.token`, which FCM accepts for an installation ID.
- `Account.register()` PUTs `{"fcm": token}` under the phone's one
  device id when the app has FCM and FCM gives a token, and otherwise
  `{"connection": true}`. Either PUT is tried again after the backoff
  while the server cannot be reached or fails for now. A refusal that
  would come again, such as the 422 of a server that takes no FCM
  devices, and a missing token both fall back to the connection.
  `Account.wakes` says which the server uses, and is unknown until an
  FCM registration settles. When the server takes the token, notifying
  begins, so a one-way notification that arrives before the first
  wake-up is news.
- `WakeService`, a `FirebaseMessagingService`, enqueues one expedited
  `WakeWorker` for each wake-up, which replaces one already waiting,
  and PUTs each registered ID through `Account.registerToken()`.
  `WakeWorker` runs `Account.catchUp()`: `Sync.refresh()`, then
  `Notifier.post()`, which makes the notifications what the copy says
  once, the same update that `Notifier.follow()` makes for the
  connection on each change. It retries while the server cannot be
  reached. On Android 11 and earlier, WorkManager runs expedited work as
  a foreground service, with a quiet notification on the connection's
  channel.
- Signing in, and the app coming to the front, start the connection
  only once the registration settles on it. The phone's boot still
  starts it, since Android lets the boot's receiver start a foreground
  service and registering decides only later; the service ends itself
  as soon as the server takes the token. A start refused because the
  app has left the screen waits for the app to come to the front.
- With FCM, Settings shows "This phone wakes through Google's push
  service when a question arrives, so transom holds no connection to
  your server in the background." in place of the background
  connection's switch. The devices list labels an `fcm` device "Wakes
  through Google's push service".
- Firebase reaches FCM through Google Play services libraries, which
  Google publishes under the Android Software Development Kit License,
  not an open-source license. The licenses screen says so.
- `@FirebaseOnly` marks the code that calls Firebase and Play services,
  `Firebase.push()` and the `Push` it returns, and Kover leaves it out:
  Robolectric has no Play services and the tests' build no Firebase
  project. The tests give the app a `Push` of their own and run
  everything else against the real server: the 422 fallback, a token
  sent again after a lost connection, a new token, wake-ups that post a
  question and a one-way notification and clear a withdrawn one, the
  connection ending once the token is taken and holding when it is
  refused, and Settings. The server refuses a token with a space, so
  the tests of a refusal register `"not a token"`.
- On a Pixel 9a with the app open a moment before, a question reached
  the phone's `WakeWorker` 2.7 seconds after it was sent, and answering
  it from the notification woke the phone again to clear it.
- With FCM, nothing posts or clears notifications between wake-ups,
  even while the app is open: reading an exchange in the app leaves
  its notification until the next wake-up.

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

- FCM's HTTP v1 API deprecates `message.token` for `message.fid`, the
  installation ID. Whether the gateway sends the ID as `fid`, and when.
- Whether the app wakes by UnifiedPush as well, for phones without
  Google services that run a UnifiedPush distributor.
