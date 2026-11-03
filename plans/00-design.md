# transom design

transom lets a program ask a person a question and get an answer back
without a chat. An agent running unattended asks "Merge this pull
request?" with the buttons Merge and Hold and a box for a typed reply.
The question arrives on the person's phone as a notification they can
answer from the notification itself, and it waits in an inbox in the app
and on the web until someone answers it or it expires. The answer goes
back to the program that asked: in the response to a waiting HTTP
request, through an MCP tool, or as a signed webhook.

transom also sends one-way notifications, like an alert from
Alertmanager or a finished download, so one server and one app replace a
hosted push service.

Everything runs on the operator's own hardware except one piece: a small
wake-up gateway that the official Android build needs in order to use
Firebase Cloud Messaging. The gateway never sees the content of a
message.

## What exists, and why transom is new

- **ntfy** has topics, a self-hosted server, an Android app, and up to
  three action buttons, but no typed reply
  ([ntfy#1089](https://github.com/binwiederhier/ntfy/issues/1089) has
  been open since April 2024), and a notification's buttons are the only
  way to act on it.
- **Pushover** and **Gotify** send one way.
- **Home Assistant's** actionable notifications can carry buttons and a
  typed reply, but the notification is the only place to answer, and it
  ties the system to a home automation server.
- **Blipr**, **Pushary**, and **HumanLayer** have the agent side but not
  a self-hosted Android path with free-text replies.

transom's question is a first-class object with a state, not a
notification with buttons bolted on. That is what lets the same question
be answered from the notification, the app's inbox, or the web, and
clear everywhere once it is.

## Concepts

### Topics

A topic is a path, like `alerts/cluster` or `agents/review`. Publishing
goes to one topic. Subscribing to a topic also subscribes to every topic
under it: a subscription to `alerts/` receives `alerts/cluster` and
`alerts/backups`. A topic exists as soon as something publishes to it.

A few first path segments are reserved for the server's own routes and
cannot start a topic name: `m`, `mcp`, `inbox`, `devices`, `login`,
`static`, `healthz`, and `metrics`.

### Messages

Every message has:

- `id`, a ULID, so ids sort by creation time.
- `topic`.
- `body`, Markdown. The notification shows the start of it, and the
  inbox shows all of it.
- `title`, optional.
- `importance`: `min`, `low`, `default`, `high`, or `urgent`. The
  Android app maps each one to a notification channel, so a person
  decides per level what sounds, vibrates, or breaks through Do Not
  Disturb.
- `tags`, a list of short strings the app and web UI can filter on.
- `link`, an optional URL that opening the message leads to.
- `deadline`, optional. After it, a notification is removed from devices
  and a question expires.
- `metadata`, an optional map of strings that transom stores and returns
  but never interprets. A caller puts its own correlation keys here,
  like the id of the agent session that asked.

A message is a **question** when it has `actions`, a `reply`, or both:

- `actions` is a list of `{id, label}`. The answer names one `id`.
- `reply` allows a typed answer, with an optional placeholder for the
  text box. A question with both lets the person pick an action, type
  text, or do both, and says which are required.
- `callback` is an optional URL that receives the outcome.

A question has a state: `open`, then exactly one of `answered`,
`expired`, or `withdrawn`. The first answer wins. A later answer gets
`409 Conflict` with the answer that won, so two people on one topic
never both think they decided.

An answer records the action id, the text, who answered, from which
device, and when.

A notification, a message with no actions or reply, has a read state for
each person instead.

### People, devices, and tokens

A server has people. Each person has a password for the web UI, any
number of paired devices, and a list of subscribed topics. Their inbox
is every message on those topics.

A **publisher token** lets a program publish, ask, and read answers on
the topic prefixes it is granted. Each token has its own signing secret
for callbacks. Tokens are random, shown once, and stored as SHA-256
hashes.

A device pairs by scanning a QR code on the web UI's devices page. The
code holds the server's URL and a one-time pairing secret, which the app
trades for a device token.

## The HTTP interface

curl is the first client, so the common cases are one command.

Send a notification:

```sh
curl -H "Authorization: Bearer $TOKEN" \
  -d "Backup finished in 4m12s" \
  https://transom.example.com/backups
```

Ask a question and wait up to an hour for the answer, printing only the
answer:

```sh
answer=$(curl -sf -H "Authorization: Bearer $TOKEN" \
  -H "Accept: text/plain" \
  -H "Actions: restart=Restart, leave=Leave it" \
  -H "Wait: 1h" \
  -d "postgres is using 94% of its disk. Restart it with the larger volume?" \
  https://transom.example.com/ops)
```

The fields that are not the body come from headers or, with the same
names, from query parameters: `Title`, `Importance`, `Tags`, `Link`,
`Deadline`, `Actions`, `Reply`, `Callback`, `Wait`, and `Meta-<key>` for
each metadata entry. transom uses `Importance` rather than `Priority`
because `Priority` is a standard HTTP header (RFC 9218) that proxies may
act on. A JSON body with `Content-Type: application/json` carries the
same fields for programs that would rather not build headers.

- Without `Wait`, a question returns `201 Created` with its id, its URL,
  and `"state": "open"`.
- With `Wait`, the request stays open until the question is answered,
  expires, or the wait runs out. An outcome returns `200` with the
  question and its answer. A wait that runs out returns `202 Accepted`
  with the question still open, and the caller can wait again.
- `Accept: text/plain` returns just the action id, or the text when the
  answer has no action, so a shell script can branch on it.
- `Idempotency-Key` makes a retried publish return the original message
  instead of asking twice. An agent that retries after a timeout would
  otherwise ask the same question again.

The other routes:

| Route | What it does |
| --- | --- |
| `GET /m/{id}` | The message and its state. HTML for a browser, JSON otherwise. |
| `GET /m/{id}?wait=10m` | The same, after waiting for an outcome. |
| `POST /m/{id}/answer` | Answer a question with `action`, `text`, or both. |
| `DELETE /m/{id}` | Withdraw an open question. Devices remove it. |
| `GET /{topic}` with `Accept: text/event-stream` | Server-Sent Events for a topic and everything under it. |
| `GET /inbox?since={cursor}` | A person's inbox changes since a cursor, for the app's sync. |
| `/mcp` | The MCP endpoint. |

## Callbacks

When a question with a `callback` reaches `answered`, `expired`, or
`withdrawn`, transom POSTs a JSON event to the callback URL:

```json
{
  "event": "answered",
  "message": { "id": "01JB…", "topic": "agents/review", "metadata": { "session": "abc" }, "…": "…" },
  "answer": { "action": "merge", "text": "squash it", "by": "chris", "at": "2026-11-03T18:04:11Z" }
}
```

- `Transom-Signature: sha256=<hex>` is the HMAC-SHA256 of the body with
  the publishing token's signing secret. It is the scheme GitHub uses in
  `X-Hub-Signature-256`, so a receiver that verifies GitHub's webhooks
  needs only a different header name.
- `Transom-Delivery` is a unique id for the delivery, so a receiver can
  drop a duplicate.
- A failed delivery is retried with exponential backoff for 24 hours.
  Each attempt is stored, so a delivery survives a server restart.

The callback is what makes a waiting agent unnecessary. An agent
framework that wakes a session on a webhook, through something like
[wicket](https://tangled.org/guid.foo/wicket), can end the session's
turn after asking. The answer arrives as an event, the framework routes
it to the session by the `metadata` it asked with, and the session wakes
with the answer in hand. Nothing holds a process open while a person is
asleep.

## The MCP endpoint

The server serves MCP over Streamable HTTP at `/mcp`. The client
authenticates with a publisher token as a bearer token. The tools:

- `notify` sends a notification.
- `ask` asks a question. It takes the same fields as the HTTP interface.
  With `wait`, it returns the outcome or, if the wait runs out, the open
  question's id. Without `wait`, it returns the id at once. While it
  waits it sends MCP progress notifications, so a client's tool timeout
  does not fire on a question that is still open.
- `get_answer` returns a question's state and answer, with an optional
  `wait`.
- `withdraw` withdraws an open question.

The tools' descriptions tell the model to ask one question at a time, to
put the context a person needs to decide into `body`, and to prefer a
callback over a long wait when its framework supports one.

## Delivery to devices

A device learns about a change through one of two paths, and in both
the app then syncs its inbox from the server with `GET /inbox`, so the
notification and the inbox always show the same state.

- **Wake-up through Firebase Cloud Messaging** is the default for the
  official build. The server POSTs to the wake-up gateway with the
  device's FCM registration token. The gateway sends a high-priority
  FCM data message that carries nothing but a sequence number. The app
  wakes, syncs, and posts or updates notifications. High-priority FCM
  is what Android lets through Doze and through manufacturers' battery
  managers, so this path is the one most likely to arrive on time.
- **A persistent connection** from the app to the server's event
  stream, for phones without Google Play services and for people who
  will not use them. It needs a foreground service, which Android shows
  as a permanent notification, and some manufacturers still stop it.

UnifiedPush is a possible third path, later.

### The wake-up gateway

FCM sends only to apps built with the sender's Firebase project, so a
self-hosted server cannot send to the official app by itself. The
gateway, run at `push.transom.sh`, is the official build's Firebase
sender. It is stateless: it takes `{fcm_token, sequence}`, rate-limits
by FCM token and by source address, and forwards. It never receives a
title, a body, or a topic.

Anyone who builds the app with their own Firebase project runs the same
gateway binary with their own credentials and points their server at
it.

A later version can encrypt a short preview to a key the device
registered, the way Web Push does (RFC 8291), so a notification can
show before the sync finishes. The first version shows the notification
after the sync.

## The Android app

The app is Kotlin, the language of Android's own APIs, with Jetpack
Compose for its screens.

- **The inbox** is the app's main screen: questions first, open before
  answered, then notifications. It is a local Room database that `GET
  /inbox` keeps in sync, so it works offline and opens instantly.
- **A question's notification is fully answerable.** Android shows at
  most three actions on a notification, and a reply box takes one of
  them. A question with a reply shows the reply box and its first two
  actions. A question with more actions than fit shows a "More" action
  that opens it in the app.
- **An answer from a notification** is queued with WorkManager, so it
  is sent when the network returns. The notification changes to show
  the answer, then clears once the server confirms it.
- **When a question is answered somewhere else, expires, or is
  withdrawn,** the next sync removes its notification and marks it in
  the inbox.
- Each importance level is a notification channel, so the person sets
  sound, vibration, and Do Not Disturb behavior per level in Android's
  settings.

The app is distributed as a signed APK on GitHub releases, which
Obtainium can track and update from. The release key is registered with
Android's developer verification before it is enforced worldwide in
2027. Google Play and F-Droid are later steps. F-Droid's main repository
does not accept the Firebase library, so its build would use the
persistent connection only.

## The web UI

The server serves a web UI at `/`: sign in, the inbox, a page for each
message where a question can be answered, a page to pair devices, and a
page to manage publisher tokens. It is HTML rendered on the server, with
a small script that listens to the event stream to update the page. A
question's page, `/m/{id}`, is also a link that any other channel, like
an email, can point at.

## Implementation

### Languages

Rust for everything except the Android app: the server and its CLI, the
wake-up gateway, and the types they share. The Android app is Kotlin.

The server is one binary, `transom`. `transom serve` runs the server,
and the other subcommands administer it: `transom user`, `transom
token`, and `transom topic`.

The server uses tokio and axum, SQLite through sqlx, the official Rust
MCP SDK (`rmcp`), maud for HTML, and reqwest with rustls for outbound
calls. rustls with webpki-roots compiled in means the binary needs no
CA certificate files at run time.

SQLite in WAL mode holds everything: people, devices, tokens, messages,
answers, and callback deliveries. A server is one process with one
database file, which is the right size for one household or one team.
Notifications are pruned after a retention period, 30 days by default.
Questions are kept.

### Layout

One repository holds everything that is built or shipped, grouped by
what it is about:

```
plans/          the designs, starting with this one
protocol/       crate: the message, question, answer, and event types
server/         crate: the transom binary, with one module per domain
                (topics, messages, answers, people, devices, callbacks,
                delivery, mcp, web)
gateway/        crate: the wake-up gateway
android/        the Android app, a Gradle project
deploy/         example Compose file and Kubernetes manifests
```

### Images

Each Rust binary is built static against musl and shipped in a `FROM
scratch` image holding only the binary, running as UID 65532. The images
are built for amd64 and arm64.

### Quality gates

Every gate runs in CI on every pull request and blocks the merge.

- **Rust:** `rustfmt`; `clippy` with warnings denied, including the
  pedantic lints; `cargo-deny` for advisories, licenses, duplicate
  versions, and crate sources; and `cargo llvm-cov` failing below 100%
  line coverage. The toolchain is pinned in `rust-toolchain.toml`.
- **Kotlin:** ktlint, detekt, and Android lint with warnings as errors.
  Kover fails below 100% line coverage for everything outside Compose
  UI code. The UI is covered by Compose tests under Robolectric.
- **Files:** [loq](https://github.com/jakekaplan/loq) holds every source
  file to 500 lines, and prek runs the formatters and linters before
  each commit.
- **End to end:** CI starts the server and an Android emulator, pairs
  the app, asks a question with curl, taps an action on the
  notification with UI Automator, and checks that curl printed the
  answer and that the callback receiver got a signed event.

### Supply chain and repository security

- Dependabot updates Cargo, Gradle, GitHub Actions, and base images.
- Every GitHub Action is pinned to a commit SHA. zizmor checks the
  workflows, and each workflow's token has only the permissions it
  needs.
- CodeQL scans Rust, Kotlin, and the workflows.
- Secret scanning with push protection, Dependabot alerts, and private
  vulnerability reporting are on. `SECURITY.md` says how to report.
- A ruleset on `main` requires every gate and a linear history.
- Releases attach build provenance attestations to the images and the
  APK. The OpenSSF Scorecard runs weekly.

## Stages

1. **The server, end to end from a browser.** Topics, notifications,
   questions, waits, answers, callbacks, the MCP endpoint, the web UI,
   and the CLI. Done when an MCP client asks a question, a person
   answers it on the web, and both the waiting tool call and a callback
   receiver get the answer.
2. **The Android app over a persistent connection.** Pairing, the
   inbox, answerable notifications, and clearing on outcomes. Done when
   the end-to-end test above passes on an emulator.
3. **The wake-up gateway and FCM.** Done when the official build gets a
   question within seconds on a phone that has been idle in Doze.
4. **Releases.** Images on GHCR, the signed APK on GitHub releases, the
   transom.sh site, and a move to a `transom-sh` organization.
5. **Integrations.** Receivers for Alertmanager and other webhook
   senders, so they can publish without a relay in front of them.

## Open questions

- Should the Android app share Rust code with the server through UniFFI,
  such as the protocol types and the sync logic? It would put more of
  the system in Rust and keep the types in one place, at the cost of a
  native library in the APK and a more involved build. The first
  version uses plain Kotlin types checked against JSON fixtures that
  `protocol/` generates.
- Is a person's password the right way into the web UI, or should the
  first version also accept an OIDC provider for people who already run
  one?
