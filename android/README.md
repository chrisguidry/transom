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
  check assembleDebug assembleRelease ktfmtFormat help
```

Then read the diff: each new checksum should belong to the version that
changed.

Robolectric runs offline, from the framework jar that
`robolectric-android-all` names in the catalog.  When Robolectric
changes, that version changes with it: it is the one Robolectric's
`DefaultSdkProvider` lists for SDK 37.
