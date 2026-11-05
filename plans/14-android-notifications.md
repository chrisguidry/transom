# 14, Android notifications

Designed, and built as step 9 of plan 09, except where Built says
otherwise.

The Android app's notifications, and the background connection that
brings what they show with the app closed. This is step 9 of
[plan 09](09-android-app.md), which keeps the rest of the app.

## Proposed

These are the design of the notifications as they are built. Each moves
to Decided when it is agreed.

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

## Built

- The notifications and the background connection are in the
  `notifications` package of `android/`. Each urgency is a channel,
  made when the app starts: `high` at high importance, so it sounds and shows a
  heads-up, `normal` at default, `low` at low, and `very-low` at
  minimum. A fifth channel, `connection`, at minimum importance, holds
  the background connection's own notification.
- `ConnectionService` is the background connection: a foreground
  service that runs while the app is signed in, follows the account's
  feed, and keeps the notifications current. It is a `remoteMessaging`
  service, the type Android describes as transferring text messages
  from one device to another. It has no time limit, and a receiver of
  `BOOT_COMPLETED` may start it. A `dataSync` service may run six hours
  a day on Android 15 and may not start from `BOOT_COMPLETED`. Google
  Play asks for each type an app declares and why; `specialUse` is the
  fallback if Play refuses `remoteMessaging`. Signing in starts the
  service, and so do the app coming to the front, the phone's boot,
  and an update of the app, through `BootReceiver`. Signing out stops
  it and cancels every notification. The service registers the phone
  as its `{"connection": true}` device, and `Account.register()` tries
  again after the feed's backoff while the server cannot be reached or
  fails for now, once per account. The feed and the registration both
  back off from a second to a minute, so a phone that cannot reach its
  server, as before Android 17 grants the local network, never retries
  in a tight loop.
- `Account.follow()` shares one feed among its callers, so the inbox on
  screen and the background connection hold one connection between
  them.
- `Notifier` makes the notifications what the copy says. `shown` decides
  each exchange's notification: one this person received and did not
  send, unread and open, and before its deadline. That is every such
  question, and each one-way notification that arrived since notifying
  began for this session. A question that arrived before then is
  posted without a sound. The `notified` table records what was
  posted, so a notification posted again after the process or the
  phone restarts makes no sound, and each is posted to alert only
  once. A notification is cancelled when its exchange ends, is read on
  any device, or leaves the copy, and `setTimeoutAfter` clears it at the
  deadline even offline.
- Each sender is a conversation: `MessagingStyle` from a `Person` named
  `@name`, not a bot, with a long-lived dynamic shortcut for each
  sender, so the long-press menu offers Priority, Default, and Silent
  for that sender. The message is the summary in bold, then the body,
  rendered by step 8's `Markdown` and turned into spans: bold, italics,
  strikethrough, and monospace code, with lists, quotes, and tables as
  lines of text. The message's images are left out.
- Reply comes first: a `RemoteInput` labelled with the prompt, or
  "Reply", that always takes text. Its chips are the choices when all
  of them fit: three at most, 30 characters in all, and no label twice.
  The emulator showed three labels of 30 characters in one row with a
  little room at the default font size. Otherwise the reply box takes
  text only, and the choices are in the app. A chip's label is mapped
  back to its choice's id. Then Open, and Open link for the message's
  first link, each set to need an unlocked phone. Tapping opens the
  exchange, which reads it, and swiping it away reads it through the
  outbox.
- An answer from the notification goes through the outbox. Android
  keeps an answered notification on screen until the app posts it
  again; a cancel before then changes nothing. About 200 ms after the
  answer, Android posts its own copy of the notification as it was,
  which replaces anything the app posted in between. A cancel that
  follows the app's update within a moment brings that copy back too.
  So the notifier leaves the exchange alone for a second, posts it with
  the answer from "Me" and no actions, and waits another second before
  it may cancel. If someone else answered first, the notification
  shows who and what until it is read.
- The notifications are not grouped by the app. On the emulator, a
  group with a summary put every conversation one more tap from its
  chips. Android bundles the app's notifications by itself anyway and
  shows the newest one expanded with its actions.
- Two parts of the design are not built: an image the body places is
  left out of the notification, and the setting that lifts the unlocked
  phone for plain actions waits for the settings screen of step 10.
- `NotificationPermission` asks for `POST_NOTIFICATIONS` once, after
  sign-in, on Android 13 and later. The top bar has the web's bell, in
  the accent while this device's notifications are on and struck
  through while they are off. Pressing it asks again while Android
  still would, and otherwise opens the app's notification settings.
- The database is version 2: Room's automatic migration adds the
  `notified` and `notifying` tables. The emulator ran it over a version
  1 install.
- The tests run the service under Robolectric against a real server:
  a question reaches Bob's notifications, a chip replies with its
  choice's id, typed text replies as text, a withdrawal, a read
  elsewhere, an answer elsewhere, and a deadline each clear it, a lost
  answer says who answered first, and swiping reads it. The emulator
  ran the heads-up, chips, a typed reply, the answer shown and then
  cleared, a withdrawal, a deadline, swiping, Open, the restart after
  an update of the app, the long-press menu, and the bell. A boot, a
  lost answer, and a read on another device ran only in the tests.
