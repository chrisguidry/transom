# 11, Releases and distribution

Partly built: the server's image is published. The app's releases come
with the app.

How the server, the gateway, and the app reach people.

## Decided

- The app ships as a signed APK on GitHub releases, which Obtainium can
  track.
- The app is registered with Android's developer verification before
  it is enforced worldwide in 2027.
- Google Play and F-Droid come later, if outside interest makes them
  worth it.
- The project moves to a `transom-sh` GitHub organization.

## Built

- Each push to `main` that passes every check publishes the server's image
  to `ghcr.io/chrisguidry/transom`, tagged with the build's UTC time and
  short commit, such as `20261104051200-8ba7cca`, which sort in build
  order. The image carries a build provenance attestation.
