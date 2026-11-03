# 01, The repository and its quality gates

Not built. Stub.

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

## Open

- How far 100% coverage can reach into the Android app's UI code.
