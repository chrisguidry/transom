# 04, Messages and questions

Not built. Stub.

What a message carries, how a question is answered, and a question's
lifecycle.

## Decided

- A question is answered with a choice, typed text, or both.
- A question has a one-line summary, an optional short version for the
  notification, a Markdown body with its context, and for each choice a
  label and an optional Markdown description.
- A question is open until it is answered, expires, or is withdrawn, and
  the first answer wins.
- A question is always answerable from its notification. The person
  decides whether they have read enough.
- The asker can attach its own data, which comes back with the answer.
- A one-way notification is the same object without choices or a reply
  box: a summary, an optional short version, and a Markdown body.
- A message can carry a link, opened when its notification is tapped,
  and an importance level. On Android each level is a notification
  channel, so each person decides what each level does.
- Images are sent with the message as attachments, and the body places
  them inline. transom serves them behind the same sign-in as the
  message. Video comes later.

## Open

- The name of this central object, which is a question when it has
  choices or a reply box and a notification when it has neither.
