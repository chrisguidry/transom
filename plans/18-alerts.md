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
