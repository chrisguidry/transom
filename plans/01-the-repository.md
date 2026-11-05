# 01, The repository and its quality gates

Built. The Rust workspace, its gates, the server's image, and the
Android app's gates are built.

The Cargo workspace, the Gradle project for the app, and the checks that
every change passes in CI.

## Decided

- One repository for everything transom builds and ships.
- Rust for everything but the Android app, which is Kotlin.
- Formatting, linting, dependency audits, and 100% line coverage, each a
  gate that blocks a merge.
- Images built `FROM scratch`, holding one static binary.
- Dependencies update automatically, GitHub Actions are pinned to commit
  SHAs, and releases carry build provenance.
- Changes are pushed to `main` for now, not reviewed in pull requests.

## Built

- `rust-toolchain.toml` pins the Rust release, and the workspace's
  `Cargo.toml` denies clippy's pedantic lints and forbids `unsafe`.
- The CI workflow runs `cargo fmt`, `cargo clippy`, `cargo deny`, and
  `cargo llvm-cov`, which fails if any line went unrun by every test,
  against a Redis service container. The gate is
  `--fail-uncovered-lines 0`, which judges each line across both the unit
  tests and the process tests, rather than a percentage, which counts a
  function's copy in each test binary separately. It also runs loq and zizmor, and builds the image.
- `android/` is one Gradle project, run by a wrapper that pins Gradle's
  release and its SHA-256. `./gradlew check` runs ktfmt in the kotlinlang
  style, Android Lint with warnings as errors, the unit tests, and Kover,
  which fails below 100% line coverage of the debug build. Kotlin
  compiler warnings are errors too. CI's Android job validates the
  wrapper jar and runs `./gradlew check` on JDK 21.
- Gradle checks every dependency against the SHA-256 checksums in
  `android/gradle/verification-metadata.xml`. That includes the Android
  framework jar that Robolectric runs, which Gradle downloads and
  Robolectric reads offline. Dependabot does not update the checksums, so
  each of its Gradle pull requests needs them written again, as
  `android/README.md` says.
- Robolectric runs the activity and its Compose screens in the unit tests
  on the JVM, drawing with Android's native graphics, so the tests can
  check pixels as well as text. The coverage gate reaches every line of
  the activity and the Compose screens this way, and excludes nothing,
  the classes the Compose compiler generates included.
- GitHub's default CodeQL setup scans the repository.
- Dependabot watches Cargo, Gradle, GitHub Actions, and the Dockerfile,
  and waits 7 days after a release before proposing it.
- prek runs the same checks before each commit, except that for the app
  it runs only ktfmt, since lint and the tests take minutes. The tests
  need a Redis at `TRANSOM_TEST_REDIS_URL`, which defaults to
  `redis://127.0.0.1:6379`.
- `server/Dockerfile` links a static musl binary with `rust-lld`.
  The final image holds only the binary and runs as UID 65532.
- Images are built for amd64 only.
- `test-issuer` is a workspace crate the server's tests use in-process
  and that also runs as a binary, for tests against a real transom. It
  is never published and is not in the image: the image build sees only
  its manifest, and an empty library stands in for its source.
