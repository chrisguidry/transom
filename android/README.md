# The Android app

The build needs JDK 21 at `JAVA_HOME` and the Android SDK at
`ANDROID_HOME`, with the `platforms;android-37.0` and
`build-tools;37.0.0` packages.  From this directory:

```
./gradlew check          # every gate: ktfmt, lint, the tests, and coverage
./gradlew ktfmtFormat    # format the Kotlin and the Gradle scripts
./gradlew assembleDebug  # app/build/outputs/apk/debug/app-debug.apk
```

The tests run on the JVM under Robolectric, with no emulator.  The
palette test reads `../server/src/web/static/transom.css`, so the app's
colors in `theme/Palette.kt` change in the same commit as the
stylesheet's.

## Testing against a real server

The tests that talk to a server run a real transom and a real
`test-issuer`, with no fakes of the API.  The `buildServer` task builds
both with cargo, into `../target/debug/`, before the tests run, so the
build also needs the Rust toolchain that `../rust-toolchain.toml` names,
through rustup.  It reruns cargo only when a file under `../server/`,
`../test-issuer/`, or the workspace's `Cargo.toml`, `Cargo.lock`, or
`rust-toolchain.toml` changes.  Cargo runs as many jobs as the machine
has cores unless `CARGO_BUILD_JOBS` says fewer:

```
CARGO_BUILD_JOBS=4 ./gradlew check
```

The tests need a Redis, at `redis://127.0.0.1:6379` unless
`TRANSOM_TEST_REDIS_URL` names another, as the server's own tests do.
Each test's server keeps its keys under a prefix of its own, so tests
never see each other's keys, and the keys stay in that Redis afterwards.

A test gets its server from the `TestServer` rule in
`app/src/test/kotlin/sh/transom/testserver/`, which says how to use
it.

## The end-to-end test

`end-to-end.sh` signs in on an emulator through the Custom Tab and
answers a question from its notification, against a transom and a
`test-issuer` it starts on this machine.  It needs the same Redis as the
tests, a running emulator with Chrome, such as Android 37 with Google's
APIs, and that emulator's serial, so it never runs on a phone that is
also connected:

```
ANDROID_SERIAL=emulator-5554 ./end-to-end.sh
```

It uninstalls the app from the emulator first, so the test starts at
the sign-in screen.

## Building with FCM

The server wakes the app through Firebase Cloud Messaging when the
build has the Firebase project's four values, the outputs of
`../gateway/firebase.tf`, as Gradle properties.  Put them in
`~/.gradle/gradle.properties`, or pass each with `-P`:

```
transom.firebase.projectId=…
transom.firebase.applicationId=…
transom.firebase.apiKey=…
transom.firebase.senderId=…
```

The app starts Firebase from them itself, so the build needs no
`google-services.json`.  A build without all four, or a phone without
Google Play services, holds a connection open to the server instead.
CI reads them from the repository's variables
`TRANSOM_FIREBASE_PROJECT_ID`, `TRANSOM_FIREBASE_APPLICATION_ID`,
`TRANSOM_FIREBASE_API_KEY`, and `TRANSOM_FIREBASE_SENDER_ID`.

## Changing a dependency

Gradle checks every dependency against the SHA-256 checksums in
`gradle/verification-metadata.xml`, and Dependabot does not update that
file.  After a version changes in `gradle/libs.versions.toml`, by hand or
in a Dependabot pull request, delete the file, so the old version's
checksums go with it, and write it again with every task that resolves
dependencies.  Write it from an empty Gradle home: with a warm cache,
Gradle skips some of the files that a fresh CI runner downloads, and CI
then fails on them.

```
rm gradle/verification-metadata.xml
GRADLE_USER_HOME=$(mktemp -d) ./gradlew --no-daemon \
  --write-verification-metadata sha256 --no-configuration-cache \
  check assembleDebug assembleDebugAndroidTest assembleRelease ktfmtFormat help
```

Then read the diff: each new checksum should belong to the version that
changed.

Robolectric runs offline, from the framework jar that
`robolectric-android-all` names in the catalog.  When Robolectric
changes, that version changes with it: it is the one Robolectric's
`DefaultSdkProvider` lists for SDK 37.
