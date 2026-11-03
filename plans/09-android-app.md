# 09, The Android app

Not built. Stub.

The inbox on the phone, and notifications that answer questions.

## Decided

- Kotlin.
- The inbox is the main screen, kept in sync with the server and stored
  locally.
- Notifications are always answerable, with up to three actions, one of
  which can be a reply box.
- A notification clears when its question is answered elsewhere,
  expires, or is withdrawn.

## Open

- Whether the app shares Rust code with the server through UniFFI.
