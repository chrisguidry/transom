# 01, The repository and its quality gates

Partly built. The Rust workspace, its gates, and the server's image are
built. The Android app's gates come with the app in plan 09.

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
  `cargo llvm-cov` with a floor of 100% of lines, against a Redis service
  container. It also runs loq and zizmor, and builds the image for amd64
  and arm64.
- CodeQL scans the Rust code and the workflows on every push and weekly.
- Dependabot watches Cargo, GitHub Actions, and the Dockerfile, and waits
  7 days after a release before proposing it.
- prek runs the same checks before each commit. The tests need a Redis
  at `TRANSOM_TEST_REDIS_URL`, which defaults to
  `redis://127.0.0.1:6379`.
- `server/Dockerfile` cross-compiles on the build machine's own
  architecture and links a static musl binary with `rust-lld`, so an
  arm64 image needs no emulation. The final image holds only the binary
  and runs as UID 65532.

## Open

- How far 100% coverage can reach into the Android app's UI code.
