# 09, The Android app

Designed, with the order of the work below. Wave 1 is built: the
server's side of signing in, read state reaching devices, `test-issuer`,
the Android project and its gates, and the notification experiment.
Wave 2 is built: signing in, and the protocol and sync.

The inbox on the phone, and notifications that answer questions.

## Decided

- Kotlin, with its own types and sync logic. It shares no code with the
  server.
- The inbox is the main screen, kept in sync with the server and stored
  locally.
- Notifications are always answerable, with up to three actions, one of
  which can be a reply box.
- A notification clears when its question is answered elsewhere,
  expires, or is withdrawn.
- The app signs in through the server, as a browser does, so people sign
  in on their issuer's own page and the app holds nothing about the
  issuer. In a Custom Tab, the server runs the confidential flow of plan
  08. When it finishes, the server sends the Custom Tab to an `https` App
  Link the app has claimed, not a custom URL scheme, with a one-time
  code. The app trades the code for a session, proving with PKCE that
  it began the sign-in, and sends the session as its bearer token. The
  session is encrypted with a key in the Android Keystore.
- The app is not a client of the issuer. An issuer such as Authentik
  gives each client its own issuer URL, and transom binds an individual
  to its issuer and subject, so a client of the app's own would make the
  phone a different individual from the browser.
- The web UI shows a QR code holding only the server's URL, so the app
  knows where to sign in. Nothing secret is in it.
- A device is a resource under its identity, at
  `/@{name}/devices/{id}`. The app makes up the id when it first signs in
  and PUTs it with a name and how to wake it: `{"fcm": token}` through the
  wake-up gateway, or `{"connection": true}` for a phone that holds a
  connection open. It PUTs again when FCM rotates the token. Only the
  identity sees or changes its devices, and a DELETE forgets one, such as
  a lost phone.

## Proposed

These are the design of the app as it will be built. Each moves to
Decided when it is agreed.

### The project

- The app lives in `android/`, beside `server/`: one Gradle project with
  one app module. Its packages follow the domains of the app, not the
  layers of a framework: `signin`, `sync`, `inbox`, `exchange`,
  `notifications`, `groups`, and `settings`.
- The application id is `sh.transom`. It can never change after the
  first release.
- The minimum SDK is 29, Android 10. The target and compile SDK are 37.
  A feature from a later release, such as requiring an unlocked phone to
  answer (API 31), is used on phones that have it.
- Jetpack Compose draws the screens, Room holds the local inbox, OkHttp
  makes the requests and reads the event streams, kotlinx.serialization
  reads the JSON, WorkManager sends what was queued offline, and
  commonmark-java parses Markdown.
- `androidx.browser` opens the Custom Tab. The app needs no OAuth
  library, since the server runs the flow.
- The gates match the server's: ktfmt for formatting, Android Lint with
  warnings as errors, Kover for 100% line coverage, Gradle's dependency
  verification for checksums, Dependabot for Gradle, and loq's 500-line
  limit. Robolectric runs the Compose and notification code in the unit
  tests, so the coverage gate can reach the UI. Any line the gate
  excludes is named in the build file with its reason.

### Signing in

- The person gives the server's URL, typed or from the web UI's QR code.
  The app opens the server's `/login` in a Custom Tab, with a PKCE
  challenge it made, and the person picks an issuer there as they would
  in a browser. Nothing about any server or issuer is built into the
  app.
- When the issuer sends the Custom Tab back to `/login/callback`, the
  server starts a session as it does for a browser. For a sign-in the app
  began, it redirects to `https://transom.sh/app/callback` with a
  one-time code instead of setting a cookie. The app POSTs the code and
  its PKCE verifier to the server and gets the session's token. A code
  works once, for a minute.
- A session's token is a bearer token wherever a token from an issuer
  is, so the app uses the same API as everything else. The server
  refreshes the issuer's tokens behind it, as it does for a browser.
  Signing out ends the session, and forgetting a device ends the
  session that registered it, so a lost phone loses its access.
- The App Link's host is fixed when the app is built, because Android
  verifies the hosts an app claims when it is installed. The app claims
  `transom.sh`, the project's own site, which is separate from the server
  people host. The server only redirects there; issuers never see that
  URL. transom.sh serves `/.well-known/assetlinks.json` from
  `site/public/`, naming each key the app is signed with. Until the
  release key exists, it names the debug key of the maintainer's
  machine, so the builds made there verify. Other development builds
  and the end-to-end test approve the link with
  `adb shell pm set-app-links-user-selection`. Someone who builds the app
  with their own key claims a domain of their own.

### Sync

- The server is the system of record, and the app keeps a copy of
  `/@{name}/exchanges` in Room. The inbox draws from the copy, so it opens
  at once and works offline.
- While the app is on screen, it follows the live feed, as the web inbox
  does.
- In the background, a foreground service follows the same feed and
  shows a quiet, permanent notification while it does. That is the
  `{"connection": true}` device, and the only way to wake until the
  gateway is built.
- Replies and read marks go into an outbox that WorkManager sends when
  the phone is online. A reply is a PUT, so sending it twice is safe.
  When someone else replied first, the server answers `409` with the
  reply that won, and the app shows it.
- Marking an exchange read on one device clears its notification on the
  others.

### Notifications

The notifications are designed for answering quickly, not for parity
with the two buttons a browser allows.

- Each urgency is a notification channel, created on first launch with
  defaults the person can change: `high` interrupts with sound, `normal`
  makes a sound, `low` is silent, and `very-low` is silent and
  minimized.
- Each sender is a conversation: the notification uses `MessagingStyle`,
  with the sender as a `Person` and a long-lived shortcut, so a person
  can make one sender a priority conversation, which interrupts Do Not
  Disturb, or silence it, from Android's own long-press menu. No sender
  is marked a bot, since transom treats people and agents the same, and
  Android offers those choices only for a person.
- The title is the sender. The message's text is the summary as a bold
  first line, then the body as styled text, and the expanded
  notification shows about nine lines of it.
- A question's choices are chips: one Reply action whose `RemoteInput`
  carries the choices' labels, with free text allowed and the prompt as
  its label. Tapping a chip replies with that choice at once, and the
  app maps the label it gets back to the choice's id. Android shows one
  row of chips, about three short labels on a phone, and drops the rest
  without a sign, so a question whose choices may not fit opens in the
  app instead of offering some of them.
- Android shows three actions at most and drops any after the third, so
  Reply always comes first. The other two can be Open and the message's
  first link.
- An answer given from the notification re-posts it, silently and
  without actions, saying what was sent, and cancels it once the server
  confirms. A plain `cancel()` from the reply's receiver leaves the
  notification on screen, because Android keeps a notification up after
  a direct reply until it is updated. When someone else answered first,
  the update says who and what they chose.
- A notification clears when its exchange ends anywhere, or is read on
  another device. `setTimeoutAfter` also clears it at the question's
  deadline, even if the phone is offline.
- Tapping opens the exchange in the app. Swiping it away marks it read,
  as plan 04 says.
- An image the body places shows in the notification, with the chips and
  actions still under it.
- Android requires unlocking the phone before a chip or the text box
  answers from the lock screen, whatever the app asks. Plain actions also
  require it, by default. A setting can lift that for plain actions
  only.
- Wear OS shows the same chips and text box with no extra work.

The notification experiment on the emulator (Android 37) found each of
these, with screenshots, before they were built. It also found:

- `BigTextStyle`, the alternative to conversations, makes the summary
  the title and shows about ten lines of the body expanded, but offers
  no choice for one sender alone.
- Android groups notifications from one app, and an action of a grouped
  notification is two taps away.
- Neither style renders Markdown, so the body is converted to plain text
  or styled spans first.

### Parity with the web UI

- The same structure: the inbox is home, with "Waiting on you" and then
  "Everything else". Each exchange has its own screen with its choices
  and text box, a group has its members, and signing in is one screen.
- The same top bar: the wordmark, the signed-in `@name`, and the bell for
  this device's notifications. Where the web has Sign out, the app has a
  menu with Settings and Sign out.
- The same look: the colors of `transom.css` in light and dark, following
  the system, the system font, 4 dp corners, 1 dp rules, small-caps
  states, and the unread dot. The app does not use Android's dynamic
  color. Nothing moves unless the person asked it to, and the app follows
  the system's animation scale.
- A unit test in the app reads `server/src/web/static/transom.css` and
  fails when the app's colors differ from it, so a change to one has to
  be a change to both.
- Markdown follows the web's rules: tables, strikethrough, and task
  lists; raw HTML shown as text; links only to `http`, `https`, `mailto`,
  and transom's own pages; and images shown as links, except the
  message's own attachments.
- Only the app has Settings: the server, the account and its name, the
  notification channels, answering from the lock screen, the background
  connection, this identity's devices, signing out, and the licenses.
- A screen both should have lands in both in the same commit. The
  devices list is the first, and the web's QR code comes with it.

### Testing against a real server

The server's tests sign tokens with an issuer that runs in-process, in
`signin/test_issuer.rs`. It moves into a workspace crate of its own,
`test-issuer`, which the server's tests still use in-process and which
also runs as a binary. The app's tests and the end-to-end test then run
against a real transom, a real Redis, and that issuer, with no fakes of
the API. The binary is never published.

## Built

- Step 1, the server's side of signing in, in `server/src/signin/app.rs`.
  The app begins at `/login?app={challenge}&callback={url}`, with the
  S256 challenge of a verifier it made. The sign-in page carries both
  on each issuer's link, and `/login/{issuer}` keeps them with the
  sign-in in progress, so they survive the trip through the issuer.
  `/login/callback` then sends the Custom Tab to the callback with
  `?code=` in place of a session cookie.
- The callback must be one of the setting `app_callbacks`, by default
  `["https://transom.sh/app/callback"]`: a YAML list, or in
  `TRANSOM_APP_CALLBACKS`, URLs separated by spaces, since a URL may
  have commas. Anything else is refused with `400`, both when the
  sign-in begins and again when it finishes.
- The code is 32 random bytes, and Redis holds only its digest, for 60
  seconds. The session does not exist until the code is redeemed, so
  Redis never holds a token the app could present. `POST /login/app`
  takes `{"code", "verifier"}` and answers `{"token"}`. A code is spent
  by its first try, right or wrong, so a verifier cannot be guessed at.
- The app's token starts with `transom_`. An issuer's JWT starts with
  `ey`, the base64url of `{"`, so the bearer token is read as one or
  the other by its prefix, and `Verified` accepts both, refreshing the
  issuer's tokens behind a session as it does for a browser.
- An app session lasts a year unused, where a browser's lasts 30 days:
  a phone that only receives pushes can go months without asking
  transom anything. The issuer still ends it at the next refresh after
  it stops signing the person in.
- `POST /logout` with the token ends the session and answers `204`.
- `{"connection": true}` is a way to wake a device, and nothing is
  pushed to it. A device PUT with the app's token keeps the digest of
  that session beside the device, apart from what the device sent, and
  deleting the device ends the session. A browser's cookie records no
  session, so turning off a browser's notifications does not sign it
  out.
- Step 2, read state reaching devices, is in plans 04 and 08: a read
  pushes `{"event": "read"}` to the reader's own devices.
- Step 3, `test-issuer`, is in plans 01 and 03.
- Step 4, the Android project, is in `android/`, and its gates are in
  plan 01. The app opens to an empty inbox titled "Inbox", in the colors
  of `transom.css`, which a unit test reads to keep the two the same.
- The app's tests run against a real server through the `TestServer`
  rule in `android/app/src/test/kotlin/sh/transom/testserver/`. Gradle's
  `buildServer` task builds `transom` and `test-issuer` with cargo before
  the tests, and the rule starts both for each test, on free ports,
  with a Redis prefix of the test's own. It gives the server's URL, a
  token for any `@name` from the issuer's `/access-token`, and the app's
  sign-in over plain HTTP, ending at the one-time code. CI's Android job
  has a Redis service and the Rust toolchain for it.
- Step 6, signing in, is in `android/app/src/main/kotlin/sh/transom/signin/`.
  The sign-in screen asks for the server's URL, and adds `https://` when
  no scheme is given and drops a trailing slash. Continue makes a PKCE
  verifier of 32 random bytes and its S256 challenge, and first asks
  the server for `/login?app={challenge}&callback=…` itself: transom's
  page carries the challenge on each issuer's link, so a page without it
  is not transom, and the person learns of a mistyped address before a
  browser opens. The app then keeps the server and the verifier on disk
  and opens the same page in a Custom Tab, so a callback that arrives
  after Android stopped the app still redeems its code.
- The app claims `https://transom.sh/app/callback` with an
  `autoVerify` intent filter. `MainActivity` is `singleTask`, so the App
  Link returns to it and closes the Custom Tab. The app POSTs the code
  and verifier to `/login/app`, then learns its `@name` from `GET /`
  with the token, reading the `303` to `/@{name}/exchanges` rather than
  following it. When the app comes back to the front with a sign-in
  still pending and no callback, the Custom Tab was closed, and the
  screen says the sign-in was cancelled. Each problem a person can meet
  has a plain message: no URL, a server that can't be reached, an
  address that isn't transom, a cancelled sign-in, a refused code, and
  no browser.
- The session, `Session(server, token, name)`, is sealed with
  AES-256-GCM in a file under `noBackupFilesDir`, as is the sign-in in
  progress. The key is made in the Android Keystore and never leaves it.
  The encryption takes the key as a parameter, and the tests run it with
  a key from the JVM, through `TestApplication` in
  `robolectric.properties`, since Robolectric has no Keystore. The only
  lines Kover excludes are the two marked `@AndroidKeystoreOnly`: the
  function that gets or makes the Keystore's key, and the application's
  call to it. The emulator ran them.
- Signed in, the app opens to the inbox, headed `@{name}` as the web's
  is, under a top bar with the wordmark, `@{name}`, and a menu with Sign
  out. Signing out forgets the session at once and POSTs `/logout` with
  the token if the server can be reached.
- Debug builds may speak plain HTTP to `127.0.0.1` only, through the
  debug variant's network security config, so a server and `test-issuer`
  on the developer's machine reach the emulator through `adb reverse`.
- Step 7, the protocol and sync, is in the `exchange` and `sync`
  packages. `Exchange`, `Message`, `Choice`, `Reply`, and the rest read
  the server's JSON with kotlinx.serialization, skipping fields they do
  not know and reading a `null` as the field's default, since the
  server writes `"attachments": null` for a message without any. A
  reply's time is `at`, as the server names it.
- `Client(server, token)` makes each request with OkHttp: the list with
  its filters, one exchange, a reply, a withdrawal, a read mark, and
  registering the phone as a device that wakes by
  `{"connection": true}`. A reply that loses to an earlier one returns
  `Replied.Lost` with the reply that won. Any other failure throws
  `Refused`, which says whether it is `transient`: a `401`, `408`, `429`,
  or `5xx` may succeed later, and anything else would fail again.
- `Client.feed(name)` follows the live feed with okhttp-sse. The feed
  sends only what changes after it connects, so `Sync.follow()` refreshes
  from the list each time it connects, and the feed's events wait in an
  unbounded buffer until the refresh is copied, so an event is never
  overwritten by a list read before it. It reconnects after a second,
  doubling to a minute at most, and counts a connection silent for 45
  seconds as lost, since the server sends a comment every 15.
- The copy is a Room database: each exchange as JSON, with its creation
  time to order by and its read state. `Store.inbox(name)` is a flow of
  the inbox split as the web splits it, `waiting` then `others`, each
  newest first, with `unread` beside them. A refresh copies the newest
  200, the most the server lists, and drops the rest, so the copy is
  never more than that. An exchange the copy has as ended is never
  reopened by a description that arrives late, since an exchange that
  ended never opens again.
- The feed gives `read` to an exchange's sender too, and
  `GET /exchanges/{id}` gives it to no one, so the copy takes whether
  this person has a read state from the list, and for an exchange it
  has not listed yet, from whether it was sent to someone else.
- `Outbox` queues replies and read marks in the same database, one of
  each per exchange, the later replacing the earlier. The inbox shows a
  queued reply at once, as `sending`, and the exchange as read and no
  longer waiting. `Outbox.deliver(client, name)` sends the queue and
  settles each on the server's answer: the server's exchange after a
  reply, the exchange with the winning reply after a `409`, and the
  server's last description after a refusal that would come again. It
  keeps what failed for now, and returns `false` so it runs again.
- `OutboxWorker` runs the delivery as unique work named `outbox`, with a
  network constraint and exponential backoff from 30 seconds. Each new
  item appends a run, so one queued while another run is sending is
  sent too. WorkManager's own factory cannot give the worker its
  session, so `OutboxWorkerFactory` makes it, and the step that wires
  the app names that factory in WorkManager's configuration and removes
  WorkManager's default initializer.
- The phone's device id is made once and kept in the same database.
- Kover excludes the classes Room generates, `*_Impl`, which hold code
  for migrations, clearing every table, and null branches the app never
  reaches. The tests run every query through the DAOs.
- `TestServer.restart()` kills transom and starts it again at the same
  address with the same keys, so a test can drop the feed's connection
  as a lost network would.

## The order of the work

Each numbered step is one commit to `main`, as plan 01 says. The steps
of a wave run at the same time, each by a subagent on a branch in its
own worktree, and each is reviewed before it is rebased onto `main` and
pushed, one at a time, with CI green before the next. One machine runs
one emulator at a time, so steps that need it take turns.

The plan carries the work between sessions: each step's commit updates
this plan's Built section, so a new session starts by reading it.

### Wave 1

1. **App sign-in on the server.** Sign-in that ends at the app's
   callback, the one-time code and its exchange, sessions as bearer
   tokens, forgetting a device ending its session, and devices that wake
   by `{"connection": true}`.
2. **Read state reaches devices.** Marking an exchange read publishes to
   the reader's feed, and pushes a clear to the reader's devices.
3. **`test-issuer`.** The test issuer as a crate and a binary, with an
   authorization endpoint that signs in at once, so the end-to-end test
   can sign in through a Custom Tab.
4. **The Android project and its gates.** `android/`, the version
   catalog, the gates, the CI job, prek's hooks, and an app that opens to
   an empty inbox in transom's colors, with the palette test and an icon
   drawn from `transom.svg`.
5. **The notification experiment.** Done, on the emulator, and not
   merged. Its findings are in Notifications above.

### Wave 2, after steps 1, 3, and 4

6. **Signing in.** The server's URL, the Custom Tab, the code exchange,
   the encrypted session, and signing out.
7. **The protocol and sync.** The API's types, the client, the copy in
   Room, the live feed, and the outbox, tested against a real server.

### Wave 3, after step 7

8. **The inbox and the exchange.** Both screens, replying, withdrawing,
   Markdown, attachments, links, and live updates.
9. **Notifications.** The channels, the notifications as the
   experiment settled them, their actions, clearing them, and the
   background connection.

### Wave 4

10. **Settings, groups, and devices.** The settings screen, a group's
    members, and the devices list in both the app and the web UI, with
    the web's QR code and the app's scanner.
11. **The end-to-end test.** CI starts an emulator, a server, and
    `test-issuer`, asks a question with curl, and answers it from the
    notification with UI Automator. This finishes stage 2.

The wake-up gateway (plan 10) and the signed APK on GitHub releases
(plan 11) follow as stages 3 and 4.

## Open

- What tapping a notification opens. Plan 00 says the whole question,
  and plan 04 says the message's first link.
- Whether the app wakes by UnifiedPush instead of `{"fcm": token}`.
  transom already sends Web Push, encrypted per RFC 8291, and
  UnifiedPush's Android connector receives Web Push. Phones with Google
  services would wake through an FCM distributor and the gateway, which
  would still see only ciphertext. Phones without them would use any
  UnifiedPush distributor, such as ntfy, instead of a connection the app
  holds itself. This changes plan 10, so it is settled before stage 3.
- How the app scans the QR code without Google's ML Kit.
- The list has no paging and no way to ask for what changed since a
  time, so a refresh reads the newest 200 exchanges and the app keeps
  only those.
