# 18, Alerts

Not built.

What transom needs before it can take over from Pushover as the place a
home lab's alerts arrive: Alertmanager, Healthchecks, Flux, a forge's
CI, and media servers, each sending through a small relay that turns
its webhook into a transom message. The relays live with the lab's
deployment, not here; this plan covers what transom gives them.

## Urgency

A fifth urgency, `urgent`, sits above `high`, for an alert that should
reach a person who is asleep or in a meeting:

| Urgency | Phone | Browser |
|---|---|---|
| `urgent` | Its own channel, which bypasses Do Not Disturb once the person grants the app that access; alarm category; full screen on the lock screen when the person allows full-screen notifications. Repeats until read or answered. | `requireInteraction`, and shown again with each repeat (`renotify`). |
| `high` | Heads-up with sound. | `requireInteraction`. |
| `normal` | Sound. | An ordinary notification. |
| `low` | Silent, with a status bar icon. | `silent`. |
| `very-low` | Silent, in the shade only. | No notification. |

Web Push's own `Urgency` header has no level above `high`, so `urgent`
sends `high` there.

An `urgent` message repeats 5, 15, and 30 minutes after it arrives
while nobody has read or answered it, and then stops repeating; the
notification stays up. Android softens repeats closer than two
minutes, so the repeats are spaced wider than that. A repeat is a wake
like the first, and the app posts it again with sound.

The Android channel for `urgent` gets its Do Not Disturb bypass when
the app creates it, because Android lets an app set that only on a
channel the person has not changed since. The app asks for notification
policy access before it creates the channel, and asks again, with a new
channel ID, if the person grants it later.

## Replacing a message

A message may carry a `key`. A new message from the same sender to the
same recipient with the same key replaces the earlier one while the
earlier one is open: the earlier exchange ends as `replaced`, and the
new one names it in `replaces`. Clients show one notification per key:
Android posts the new message under the earlier one's notification ID,
and a browser uses the key as the notification's `tag`. An alert's
resolution replaces its firing message this way, and a burst of media
imports collapses into one message.

If the person already read or dismissed the earlier message, the new
one still arrives, and its urgency decides whether it makes a sound. A
resolution is `low`, so it updates the inbox quietly.

## Links

A link keeps the `title` parameter of RFC 8288, which the clients use
as the link's button label and link text: "View run" rather than
"Open link".

## Pictures

An identity has an optional display name and picture. Whoever manages
the identity sets them; the picture is a square PNG, JPEG, or WebP of
at most 256 KiB, served at `/@name/picture`. The exchange view gives
each party's display name beside its handle.

- Android draws the picture as the sender's `Person` icon and as the
  icon of the sender's conversation shortcut, with a dot for the
  message's urgency: red for `urgent` and `high`, amber for `normal`,
  none below.
- The web shows it on each inbox row and exchange page, and as the
  Web Push notification's `icon`.

transom refuses a picture that is not square, with `422`, rather than
taking any shape and leaving clients to crop it. A relay's owner sees
the refusal once, when setting the picture, and every client then draws
the same circle without guessing which part of a wide logo matters.
transom reads the width and height from the image's header without
decoding it, and also refuses a side over 1024 pixels: a phone decodes
the whole bitmap to draw even a small icon, and a 256 KiB file can
otherwise describe an enormous one.

Fetching a picture takes a signed-in caller, as describing an identity
does. Browsers differ on whether they fetch a notification's `icon`
with the site's cookies, so the service worker fetches the picture
itself, with the session's cookie, and gives the notification a
`data:` URL. The app fetches with its session token. The picture's URL
carries a version, a hash of its bytes, so a client that caches by URL
sees a new picture as a new URL; the response's `ETag` is the same
hash, for a client that revalidates.

The exchange view keeps `from` and `to` as handles and adds
`from_name`, `from_picture`, `to_name`, and `to_picture`, and the
reply's `by_name` and `by_picture`, each `null` when unset, so a client
that reads only the handles is unchanged.

## The read list

Unread messages show at full size with the picture and the first line
of the body. Read messages shrink to one muted line with a faded
picture.

`POST /@name/read` marks many exchanges read at once, by a list of IDs
or everything up to a time, and wakes each device once to clear the
notifications. Both clients have "Mark all read" on the unread section
and a way to mark one message read: a swipe on the phone, a button on
the web. Dismissing a browser notification marks it read, as a swipe
does on the phone. A message that retention deletes closes its browser
notification.

## Images in notifications

The first image attachment shows in the expanded notification on
Android, and as the Web Push notification's `image`.

A replaced exchange is kept until retention deletes it, as any other
exchange that ended is, so a link to it keeps working until then.

The browser's tag is the key together with the sender and the
recipient, since two senders may each use a key of the same name.

## A person's own messages

A person who signs in to an MCP client, such as a claude.ai connector,
sends as themselves, while deployed agents send as identities of their
own. transom treats a person's own messages as sent ones: they do not
notify, and a question to oneself is not "waiting on you". The connector
is for reading and answering, so this stands for now. When something
asks its person questions through it, a message to oneself sent over
MCP will count as arriving: it notifies, and a question shows as waiting
on its recipient.

## On Android

What the app settled while building its side.

- The urgent channel's id is `urgent` while the app has no access to
  Do Not Disturb. Once the person grants it, the app makes
  `urgent-dnd` with the override and deletes `urgent`; it checks each
  time the app starts or comes back to the front, so returning from
  Android's access list is enough. `urgent-dnd` stays if the access is
  later taken away, so what the person set on it is kept. Android
  restores a deleted channel with its old settings when an app creates
  its id again, so neither id is ever reused for other settings. Both
  are at high importance with the default sound; neither plays at
  alarm volume.
- An urgent notification is `CATEGORY_ALARM` and carries a full-screen
  intent when it alerts, never on a quiet update, and only while
  `canUseFullScreenIntent()` allows it on Android 14 and later.
  Without it, Android shows the usual heads-up. Settings has a line
  for each of the two grants, with Allow opening Android's own page:
  `ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS` and
  `ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT`. The app asks in Settings
  only, not when the first urgent message arrives.
- The full-screen page is `AlertActivity`, which shows over the lock
  screen and turns the screen on, in a task of its own. It shows the
  time, the sender's picture and dot, the summary, the body,
  Acknowledge, the question's choices, and the first link, in the
  design's dark red whatever the theme. Acknowledge reads the exchange
  through the outbox without unlocking, as a swipe does. A choice and
  the link ask to unlock first. The page closes once the exchange is
  no longer open, unread, and waiting, wherever that happened.
- The `notified` table, at database version 3, keeps each
  notification's tag and how many repeats it alerted for. A repeat
  that raises the count while the exchange is unread posts it again
  without `setOnlyAlertOnce`, so it sounds; one counted before a
  restart does not sound again.
- A message whose `replaces` names an exchange whose notification
  Android still shows posts under that notification's tag. It is quiet
  at `low` and below, and alerts again above. Once the earlier one was
  swiped away, the new one is a notification of its own. The feed may
  bring the earlier one's `replaced` state before the new message, so
  a replaced exchange's notification waits for its replacement rather
  than clearing and coming back.
- The list returns replaced exchanges with the state `replaced`. The
  app's inbox leaves out each replaced exchange, and each one that a
  later exchange in the copy names in `replaces`. Mark all read marks
  the unread section and the replaced exchanges never read. The web's
  inbox still lists a replaced exchange, so the two differ there.
- Pictures are fetched from the session's own server, by the path and
  version of the URL the view gives, and kept on disk under the cache,
  one file per URL; a new version is a new URL, so a kept file never
  needs the `ETag`. Signing out deletes them. The notification's icon
  is drawn at 192 pixels: the picture in a circle, or the initials on
  `--soft`, with the dot cut out at the bottom right, red (`#c2412d`)
  for `urgent` and `high` and amber (`#b7791f`) for `normal`. The
  conversation shortcut gets the same icon and the display name, and
  is pushed again when either changes.
- The first image attachment is a second message in the conversation,
  with the image as its data, through a `FileProvider` that Android
  grants the system UI as it posts the notification. `BigPictureStyle`
  would replace `MessagingStyle`, and the sender's conversation
  settings would go with it.
- A link's title labels its notification action and its line on the
  exchange's page.
- The inbox is the web's three sections. Unread rows have the picture,
  the summary, the body's first line, and the meta line; read rows are
  one muted line with a 28 dp grey picture. Swiping an unread row
  toward the start marks it read, and a screen reader has the same as
  an action. Mark all read and every other read go through the outbox,
  which sends all of them as one `POST /@name/read`.
- After a bulk read, the feed brings each read to a phone that holds
  the connection, and a woken phone's catch-up lists the exchanges
  again, so either way every notification of an exchange now read is
  cancelled.
