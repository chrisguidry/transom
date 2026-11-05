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
of the body. Read messages are two quiet lines with a faded picture,
as "The inbox" below describes.

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

## The inbox

The web's `web/inbox.rs` and the app's `Inbox` follow the same rules,
from a UX review of the first screenshots and Chris's decisions on it.

- Three sections: "Waiting on you · N", "Unread · N" with Mark all
  read, and "Earlier". Earlier shows the last week, 50 rows at most,
  and "Show N older" opens the rest.
- A question waits on you while it is open, someone else asked it,
  and it asks for an answer: it has a prompt, or it has no links. A
  question with choices and links and no prompt is an alert whose
  choices are actions, such as Silence beside a dashboard: nothing
  breaks if no one takes them, so it sits in Unread and then Earlier.
  The server's `question` flag is unchanged; the rule is the clients'.
- Left out: an exchange that ended `replaced`, one that a later
  exchange in the list names in `replaces`, and a request from
  `@transom` that was answered, such as a sender's consent request,
  since its outcome is the sender's own message in the list. Mark all
  read also marks the replaced ones never read.
- Unread and Earlier show the exchanges one sender sent one recipient
  with the same key as one row, the newest, with "3 times" in its meta
  line, so an alert that fired and resolved, or repeated after its
  earlier one ended, is one row.
- A waiting or unread row is full size: a 40 dp picture with the
  urgency's dot while unread (`--fire` for urgent and high, `--warn`
  for normal, ringed in the paper's color), the title bold with the
  dot of the accent while unread and the time right-aligned, the
  body's first line without its Markdown, and the meta line. A short
  question, three choices at most and no prompt, has its choices as
  buttons under the row. Any other row is two quiet lines: the title
  with its time, then the meta line, with a 32 dp grey picture, at
  least 60 dp tall.
- The meta line says who and where it stands in words: `Alertmanager`,
  `Alertmanager · to Ops`, `Claude · You answered: Raise and restart`,
  `Claude · Expired unanswered`, `To Claude · Waiting for an answer`.
  The time is relative, in the reader's own time zone: `now`, `4m`,
  `3h`, `Yesterday`, a weekday, then the date. The web writes it in
  UTC and its script says it again in the browser's zone.
- Nothing ever received says "Nothing here yet. Messages sent to
  @name show up here." Nothing waiting or unread says "You're all
  caught up."
- `@transom` carries the wordmark's T on the accent as its picture,
  which the server sets on its built-in identity as it starts.

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
  Without it, Android shows the usual heads-up. Settings groups the
  two grants under "Urgent alerts", each a row that says where it
  stands, with a check once granted and a filled Allow until then,
  which opens Android's own page:
  `ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS` and
  `ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT`. The app asks in Settings
  only, not when the first urgent message arrives.
- The full-screen page is `AlertActivity`, which shows over the lock
  screen and turns the screen on, in a task of its own, in the
  design's dark red whatever the theme. At the top: the time, the
  sender's picture, the sender and how long ago it arrived, the
  summary, and the body, left-aligned once it runs past two lines. In
  the bottom third, within reach of a thumb: when it rings again,
  until the last repeat; the question's choices and the first link,
  48 dp tall; Acknowledge, full width and 56 dp tall; and Open in
  transom. The picture has no dot, since the whole page says the
  message is urgent. Acknowledge reads the exchange through the outbox
  without unlocking, as a swipe does. A choice, the link, and Open in
  transom ask to unlock first. The page closes once the exchange is no
  longer open, unread, and waiting, wherever that happened.
- The shade's buttons, three at most, follow what the message asks.
  Tapping the notification opens the exchange, so no button does.
  An urgent message leads with Acknowledge, which needs no unlocking.
  A question with a prompt has a reply box labelled with the prompt,
  with its choices as chips when they fit; that is the only time the
  reply box takes text. Without a prompt, each choice is a button, or
  Answer… opens the exchange when they do not all fit. Then the links,
  each by its title. An alert whose choices sit beside links, such as
  Silence beside a dashboard, leads with its first link. Every other
  button needs an unlocked phone, which Android asks for before any
  activity a notification starts, so the setting that let plain
  buttons work from the lock screen did nothing once Open was gone,
  and it went. An urgent notification says "Rang 2 of 4 times" in its
  header.
- Settings says when the phone last heard from its server: "now"
  while the live feed is connected, and otherwise how long ago the
  last feed event or wake-up came. The server sends nothing to the
  phone for this alone.
- The `notified` table, at database version 3, keeps each
  notification's tag and how many repeats it alerted for. A repeat
  that raises the count while the exchange is unread posts it again
  without `setOnlyAlertOnce`, so it sounds; one counted before a
  restart does not sound again.
- A message whose `replaces` names an exchange whose notification
  Android still shows posts under that notification's tag. It is quiet
  at `low` and below, and alerts again above. A quiet one cancels the
  earlier notification before it posts, since Android otherwise keeps
  the earlier one's bell beside the time. Once the earlier one was
  swiped away, the new one is a notification of its own. The feed may
  bring the earlier one's `replaced` state before the new message, so
  a replaced exchange's notification waits for its replacement rather
  than clearing and coming back.
- Pictures are fetched from the session's own server, by the path and
  version of the URL the view gives, and kept on disk under the cache,
  one file per URL; a new version is a new URL, so a kept file never
  needs the `ETag`. Signing out deletes them. The notification's icon
  is drawn at 192 pixels: the picture in a circle, or the initials on
  `--soft`. It carries no urgency dot: the shade's bell, the channel,
  and an urgent message's alarm already say it, and Android draws the
  app's badge where the dot would go. The conversation shortcut gets
  the same icon and the display name, and is pushed again when either
  changes.
- The first image attachment is a message of its own in the
  conversation, before the text, since the emulator showed a tall image
  after the text pushing the summary out of the expanded notification.
  It carries the image as its data, through a `FileProvider` that Android
  grants the system UI as it posts the notification. `BigPictureStyle`
  would replace `MessagingStyle`, and the sender's conversation
  settings would go with it.
- A link's title labels its notification action and its line on the
  exchange's page.
- The inbox follows "The inbox" above. Swiping an unread row toward
  the start, over the accent with a check and Mark read, marks it
  read, and a screen reader has the same as an action; Undo shows for
  six seconds after it and after Mark all read, and marks each unread
  again. Every read goes through the outbox, which sends all of them
  as one `POST /@name/read`. The top bar leaves out the `@name` on a
  phone, where the inbox's heading names it.
- After a bulk read, the feed brings each read to a phone that holds
  the connection, and a woken phone's catch-up lists the exchanges
  again, so either way every notification of an exchange now read is
  cancelled.
