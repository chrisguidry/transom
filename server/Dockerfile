# syntax=docker/dockerfile:1

# On Alpine the host is already musl, and rustc links musl binaries
# statically by default.
FROM rust:1.99.1-alpine@sha256:7e9e2d068a107a2adafd19930011178d14a78326053cd4c3a9f7ad3fd049557d AS build
WORKDIR /src

# The dependencies build in a layer of their own, against a stub
# main.rs, so a cached build reuses them until a manifest or Cargo.lock
# changes. The registry and target directory stay in the layer, not in
# cache mounts, so the build below finds them unchanged.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY server/Cargo.toml server/
# Cargo loads every member of the workspace, so test-issuer and the
# gateway need their manifests and a target. The image never builds
# them, so an empty library and an empty main stand in for their source,
# which stays out of the build context.
COPY test-issuer/Cargo.toml test-issuer/
COPY gateway/Cargo.toml gateway/
RUN mkdir server/src test-issuer/src gateway/src \
    && echo 'fn main() {}' > server/src/main.rs \
    && touch test-issuer/src/lib.rs \
    && echo 'fn main() {}' > gateway/src/main.rs \
    && cargo build --locked --release --package transom \
      --target x86_64-unknown-linux-musl \
    && rm -r server/src

COPY . .
# COPY keeps the files' times from the checkout, which can be older than
# the stub's build, so touching main.rs makes cargo rebuild transom.
RUN touch server/src/main.rs \
    && cargo build --locked --release --package transom \
      --target x86_64-unknown-linux-musl \
    && cp target/x86_64-unknown-linux-musl/release/transom /transom

# The image holds the binary and the public CA certificates it needs to
# fetch an issuer's keys over TLS, and nothing else.
FROM scratch
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build /transom /transom
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/transom"]
CMD ["serve"]
