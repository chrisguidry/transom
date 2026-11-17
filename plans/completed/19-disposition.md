# 19, Disposition

Closed 2026-11-17. Built: all of it.

What kind of news a message is, as a field of the message, and the badge
that the clients draw for it.

## The problem

A person scans a list of messages for the ones that say something is
wrong. Nothing in a message said that. Senders put a colored circle
emoji at the start of the summary: red for a problem, green for its
resolution, blue for something to know. The only mark transom drew
itself was the urgency's dot of plan 18, on an unread row's picture.
The urgency says how a message is delivered, not what it says: a
resolution is good news sent at `low`, and a problem is bad news at
`normal` or at `urgent`. The dot also went away once the row was read,
so a list of read rows was grey apart from the senders' emoji.

## The field

`disposition` is a field of the message, beside `urgency`. Its sender
sets it, in any form a message takes: a JSON or YAML body, a form, the
query string, the `Disposition` header, and the MCP tools `ask` and
`notify`.

| Disposition | The message says |
|---|---|
| `informational` | Something to know. |
| `problem` | Something is wrong. |
| `resolved` | A problem has ended. |
| `question` | Something needs an answer. |

- Any other value gets `400 Bad Request`, as an unknown urgency does.
- A message whose sender gives no disposition is a `question` when it
  has choices or a prompt, and `informational` otherwise.
- The sender's own disposition wins over that. An alert that offers
  "Silence 1h" as a choice is a `problem`, and a message with no
  choices may be a `question`, since any message takes a typed reply.
- The stored message keeps only a disposition its sender gave. The
  exchange's JSON and the Web Push payload always carry one, so a
  message stored before this plan reads as the same default.
- The urgency is unchanged, and only decides how a message is
  delivered.

A warning and a critical alert are both a `problem`. How much each one
interrupts is its urgency.

## The badge

The web UI and the Android app draw the same badge.

- It sits on the upper right of the sender's picture, a disc 18 px
  across on a 40 px picture and 16 px on a read row's 32 px picture,
  5 px outside the picture's corner, with a ring of 2 px in the paper's
  color. The lower right is left clear: some pictures carry a mark of
  their own there, and Android draws the app's badge there on a
  conversation's icon.
- Each badge has a glyph as well as a color, so red and green are told
  apart without color. The glyph is a stroke of 1.6 units with round
  ends, in a box of 12 units, in `--accent-ink`. Both clients draw it
  from the same SVG path data.

| Disposition | Color | Glyph | Path |
|---|---|---|---|
| `problem` | `--fire` | `!` | `M6 2.9v3.9M6 9.1h.01` |
| `resolved` | `--good` | check mark | `M3.2 6.3l1.9 1.9 3.7-4.1` |
| `question` | `--accent` | `?` | `M4.3 4.7a1.75 1.75 0 1 1 2.6 1.5c-.6.35-.9.7-.9 1.2M6 9.5h.01` |
| `informational` | none | | |

- The badge keeps its color when the row is read and its picture turns
  grey, so a list scans by its badges.
- A row that stands for several exchanges with one key shows the
  newest one's badge, so an alert that fired and resolved shows the
  check mark.
- The exchange's page has the same badge on its sender's picture.
- A screen reader says the badge's word before the row: "Problem",
  "Resolved", or "Question".
- The urgency's dot of plan 18 is gone, and `--warn` with it, which
  nothing else used. `--good` is new: `#2f7d4f`, and `#7cc796` in the
  dark. An unread row is still told by its bold title, the accent's dot
  before it, and its full size.

## Notifications

- Android draws the badge on the sender's `Person` icon and on the icon
  of the sender's conversation shortcut, in the same proportions, at
  the upper right of the 192-pixel icon. The ring is cut clear of the
  picture instead of drawn in the paper's color, since the icon shows
  on the shade's own background. The shortcut is pushed again when the
  badge changes.
- The service worker draws the same badge on the Web Push
  notification's icon, with an `OffscreenCanvas` and a white ring. A
  browser that cannot draw in a worker shows the picture without its
  badge.
- A notification's icon is drawn once for both themes, so its badge has
  the light palette's colors.

## Built

- `Disposition` in `server/src/exchanges/message.rs`, parsed with the
  other fields in `fields.rs`. `Message::disposition` gives the default,
  and `message_view` writes it into every exchange's JSON.
- `Badge` in `server/src/web/pages.rs`, drawn by
  `server/templates/badge.html` and `.badge` in `transom.css`, and by
  `badged` in `sw.js`.
- `Disposition` in the app's `exchange/Message.kt`, and
  `Exchange.disposition`, which gives a copy kept from before this plan
  the server's default. `pictures/Badge.kt` draws the badge in Compose
  and on a notification's bitmap. `PaletteTest` holds the app's colors
  to the stylesheet's.
- The `send-a-notification` and `ask-a-question` skills say when to set
  each disposition.
