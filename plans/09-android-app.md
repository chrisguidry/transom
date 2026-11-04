# 09, The Android app

Not built. Stub.

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
- The app signs in as RFC 8252, OAuth 2.0 for Native Apps, describes: a
  public client running the authorization code flow with PKCE in a Custom
  Tab, so people sign in on their issuer's own page. The redirect returns
  on an `https` App Link the app has claimed, not a custom URL scheme,
  and the tokens are encrypted with a key in the Android Keystore.
- The web UI shows a QR code holding only the server's URL, so the app
  knows where to sign in. Nothing secret is in it.
- A device is a resource under its identity, at
  `/@{name}/devices/{id}`. The app makes up the id when it first signs in
  and PUTs it with a name and how to wake it: `{"fcm": token}` through the
  wake-up gateway, or `{"connection": true}` for a phone that holds a
  connection open. It PUTs again when FCM rotates the token. Only the
  identity sees or changes its devices, and a DELETE forgets one, such as
  a lost phone.

## Open

- Where the App Link's `assetlinks.json` is hosted: on transom.sh, or on
  each server.
