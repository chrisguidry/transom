# 11, Releases and distribution

Designed, with the order of the work below. The server's and the
gateway's images are published.

How the server, the gateway, and the app reach people: everything
transom publishes, from every push to `main`. Android's developer
verification and the app stores are plan 15.

## Decided

- The app ships as a signed APK on GitHub releases, which Obtainium can
  track.
- Every push to `main` that passes every check publishes what it
  changed. Nothing is published by hand.
- Each published thing carries a build provenance attestation, so
  anyone can check that this repository's CI built it from a given
  commit.

## Proposed

### The app

- Each push to `main` that passes the Android checks and the end-to-end
  test publishes a GitHub release, tagged with the build's UTC time and
  short commit like the images, such as `20261105123000-826660a`. It is
  a full release, not a prerelease, so Obtainium takes it.
- Its assets are `transom-<tag>.apk`, signed with the release key under
  APK signature schemes v2 and v3, and `SHA256SUMS`. The APK carries a
  build provenance attestation, which
  `gh attestation verify transom-<tag>.apk --repo chrisguidry/transom`
  checks.
- The release notes give the commit's subject and the signing
  certificate's SHA-256 fingerprint,
  `43:9B:0D:00:20:3D:3C:E2:04:F3:A0:11:D0:52:52:9A:F0:ED:B2:7E:6B:5A:38:F4:8A:2C:50:4F:50:41:DB:37`,
  which the job checks against the signed APK before it publishes, so a
  wrong key never publishes.
- `versionName` is the tag, and `versionCode` is the build's time in
  whole minutes since the Unix epoch, so each release's is higher than
  the one before. A local build is `dev`, version code 1.
- The release key is an RSA 4096 PKCS12 keystore with the alias
  `transom`, valid for 100 years. CI holds it as two secrets of the
  GitHub environment `release`, which only `main` deploys to:
  `TRANSOM_RELEASE_KEYSTORE`, the keystore in base64, and
  `TRANSOM_RELEASE_KEYSTORE_PASSWORD`. The job writes the keystore to
  the runner's temporary directory only for signing and deletes it
  after. Losing the key means an installed app can never be updated, so
  a copy is kept offline as well.
- The release key's fingerprint is in Firebase's
  `android_signing_sha256`, in transom.sh's `assetlinks.json` for
  sign-in's App Link, and in `android/README.md` for checking an APK by
  hand.

## Built

- Each push to `main` that passes every check publishes the server's image
  to `ghcr.io/chrisguidry/transom`, tagged with the build's UTC time and
  short commit, such as `20261104051200-8ba7cca`, which sort in build
  order. The image carries a build provenance attestation.
- The gateway publishes `ghcr.io/chrisguidry/transom-gateway`, with
  build provenance, and its manifests as
  `ghcr.io/chrisguidry/transom-gateway-deploy`, at the same kind of
  tag, as plan 10 says.
- transom.sh is built from `site/` and published as
  `ghcr.io/chrisguidry/transom-site-deploy`.

## The order of the work

1. **The release key**, made, kept in the `release` environment, and
   its fingerprint added to Firebase and `assetlinks.json`.
2. **The app's releases**: the signed build, versioning, the release
   job, its attestation, and how to verify and install an APK.
