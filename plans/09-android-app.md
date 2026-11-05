# 09, The Android app

Built: all four waves of the work below, which finish stage 2.

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
- The app reads the code with open-source libraries rather than Google's
  ML Kit, which is not open source: CameraX shows the preview and hands
  each frame to ZXing's `core`, a plain Java library.
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
  `site/public/`, naming the release key, the only key whose builds
  verify. Development builds and the end-to-end test approve the link
  with
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

The notifications, from their design to what step 9 built, are in
[plan 14](14-android-notifications.md).

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
- Android 17 refuses connections to the local network, where a home
  server usually is, until the person grants `ACCESS_LOCAL_NETWORK`.
  When a server whose address is not public can't be reached, the
  sign-in screen asks for it once per Continue and tries again on a
  grant, or says how to allow it in the app's settings. A smaller line
  under "can't be reached" gives the cause, such as a timeout, an
  untrusted certificate, or `ECONNREFUSED`, for screenshots.
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
  newest first, with `unread` beside them. A refresh copies the server's
  whole list, with no limit, since the server's retention bounds it, and
  drops each exchange the server no longer lists, such as one of a group
  this person left or one the server deleted for its age. It drops them
  one id at a time, since Android 10's SQLite takes at most 999 values in
  one statement. The feed's `gone` drops one at once. An exchange the
  copy has as ended is never
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
- Step 8, the inbox and the exchange, is in the `inbox` and `exchange`
  packages, and the wiring they share is `Account`, in `Account.kt`. An
  `Account` is what the app does as one session: its `Client`, made with
  the application's one `OkHttpClient`, its `Store`, `Sync`, and
  `Outbox`. The application builds it from the stored session the first
  time it is asked for, since WorkManager may start the process for the
  outbox alone, and `forget()` ends it on sign-out. The feed, the
  outbox's deliveries, and the device's registration run in the
  account's own scope, so `forget()` stops them before it clears the
  copy and the queue. It keeps the phone's device id, so the next
  sign-in registers the same device again. A sign-in registers the phone
  as a `{"connection": true}` device, named by `Build.MODEL`, once; a
  registration that fails is not tried again until the next sign-in.
- The manifest removes WorkManager's default initializer, and
  `TransomApplication` is WorkManager's `Configuration.Provider`, with
  `OutboxWorkerFactory` delivering as the current account.
- While the app is at least started, `SignedInScreens` follows the live
  feed, through `repeatOnLifecycle`. The navigator is one saved nullable
  id, the exchange open over the inbox, and a `BackHandler`, rather than
  navigation-compose, since two screens and one argument need no routes
  or back stack. The wordmark also goes back to the inbox, as the web's
  links home. The inbox is read above the navigator, so it is whole when
  it comes back and keeps its scroll position.
- The inbox is the web's: `@name`, then "Waiting on you" and "Everything
  else" as uppercase muted labels, each card the summary, bold after the
  accent's dot while unread, over `from → to · when · state`, the state
  in small caps, and "Nothing here yet." when it is empty. A card's
  title is the summary, or the first line of the body cut at 80
  characters, and a time is `2026-11-03 21:14 UTC`, as `pages.rs` makes
  them. An unread card says so to a screen reader. There is no pull to
  refresh: the feed refreshes from the list each time it connects.
- The exchange is `exchange.html`: the meta line, the title, the body,
  the attachments the body did not place, the links, and then the reply,
  or for a recipient of an open exchange, the choices as buttons with
  each description after them, the prompt or "Add a note, if you like"
  over a text box, and Reply, which sends the text alone. Only a
  recipient has a read state in the copy, so that is how the screen
  knows who answers. Opening it marks it read through the outbox. A
  reply goes through the outbox: it shows at once as `@name replied ·
  sending`, and the server's answer settles it, with someone else's
  reply when theirs came first. The sender sees a quiet Withdraw while
  the exchange is open or pending, which is sent at once rather than
  queued, and says so when the server cannot be reached or refuses.
- `Markdown` renders a body with commonmark-java and its tables,
  strikethrough, and task-list extensions into blocks of
  `AnnotatedString`s with link annotations, and `MarkdownBlocks` draws
  them in Compose. Its rules are `markdown.rs`'s, tested case by case in
  `MarkdownTest`: raw HTML as text; links only to `http`, `https`,
  `mailto`, and paths of transom's own, which resolve against the
  session's server; and an image as a link to it, except one of the
  message's own attachments placed directly in a paragraph, which
  splits the paragraph there. A heading of the second level is drawn as
  `transom.css` draws every `h2`, as a section's label.
- `AttachedImage` fetches an attachment with `Client.attachment(id,
  name)`, which builds the path from the exchange's id and the name, as
  the web does, so the bearer token only goes to the session's server.
  It decodes at a fraction of the size when the image is wider than
  2048 pixels, shows it as wide as it is or as the column, and shows its
  name when it cannot be loaded.
- The tests drive both screens with Robolectric against a real server
  through `TestServer`. `LaunchedApp` launches the app signed in and
  forgets its account after each test, so no feed follows a stopped
  server into the next test. `TestApplication` gives each test a test
  WorkManager with the app's configuration, and `online()` lets the
  outbox's waiting work run.
- The emulator ran the whole of it: signing in as `@chris` through the
  Custom Tab, the device's registration, and each kind of exchange in
  light and dark.
- Step 9, the notifications and the background connection, is in
  [plan 14](14-android-notifications.md).
- Step 10's web half, the devices page and its QR code, is in plan 08.
- The scanner is in `signin/Scanner.kt`: Scan beside the server's field
  asks for the camera when pressed, and a code's `http` or `https` URL
  fills in the field for Continue. Kover excludes `PhoneCamera`, the
  CameraX lines, which the emulator ran.
- Step 11, the end-to-end test, is in plan 01. It finishes stage 2.
- Step 10's app half, Settings, devices, and a group's members, is in
  plan 14's Settings section.

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

## Settled

- Tapping a notification opens the exchange in the app, and its Open
  link action opens the message's first link, as plan 14 says.
- UnifiedPush is plan 16.
