# 23, One release

Closed 2026-11-19. Built: all of it.

Everything transom ships is released together, under one tag.

## The problem

transom ships four things: the server's image, the wake-up gateway's
image, the Android app, and the Linux desktop app. Each was released
when a push to `main` touched its own files, under a tag made when its
own job ran. A server and an app built minutes apart had different
tags, and nothing said which server an app was built with.

That matters when an app needs something from the server. The desktop
app signs in through a callback that the server's default settings
gained in the same change as the app. A person who ran the new app
against an older server got a sign-in the server refused, and had no
version to compare.

Two releases in one repository also had to share GitHub's "latest".
Obtainium follows the latest release for the APK, so a release holding
only the desktop app's package could not be the latest, and whatever
followed the package had to find it by a tag prefix.

## Decided

- One tag for everything: the run's UTC time and the short commit, such
  as `20261119153000-826660a`. `ci.yaml`'s changes job makes it once,
  and every workflow that builds something to ship takes it as an
  input.
- A push to `main` that touches the server, the gateway, the Android
  app, or the desktop app releases all four, each built from that
  commit. A component whose own files did not change is built again
  and gets the new tag, so a tag always names one server and the apps
  built with it.
- A push that touches none of the four releases nothing. The site and
  Terraform deploy by themselves, as before.
- One GitHub release for each tag, marked as the latest. It holds
  `transom-<tag>.apk`, `transom-desktop_<tag>_amd64.deb`, and
  `SHA256SUMS`, and its notes name both images.
- Each workflow builds, signs, and attests what it ships. The Android
  and Desktop workflows hand their files to `ci.yaml`'s release job as
  artifacts, and that job alone creates the release. It needs every
  shipped area's workflow to have succeeded, so a release never holds
  part of what shipped.
- A run started by hand on `main` releases all four, for a push whose
  run GitHub never started.

## What it costs

- Every release updates everything. A change to the server alone gives
  each phone a new APK, and a change to an app alone rolls the server
  wherever something follows its image. The code of the untouched
  component is the same; its version moves.
- Each release builds all four, which takes longer than building one.
- An image is pushed by its own workflow before the release job runs.
  When another area then fails, the image exists under a tag that has
  no GitHub release.

## Built

- `ci.yaml`: the changes job's `release` and `tag` outputs, the inputs
  it passes to the Rust, Android, Desktop, and Gateway workflows, and
  the release job.
- `rust.yaml` and `gateway.yaml` publish their images under the tag
  they are given, and only for a release.
- `android.yaml` builds the signed APK for a release and reports the
  signing certificate's fingerprint. Its version code is the tag's time
  in whole minutes.
- `desktop.yaml` builds the package for a release, with the tag in its
  version.
