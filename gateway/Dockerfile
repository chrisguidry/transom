# syntax=docker/dockerfile:1

# Built from the root of the repository, which is the workspace:
# docker build -f gateway/Dockerfile .
# BuildKit reads gateway/Dockerfile.dockerignore in place of the root's
# .dockerignore, so this build sees the gateway's source and only the
# manifests of the other members.

# On Alpine the host is already musl, and rustc links musl binaries
# statically by default.
FROM rust:1.99.1-alpine@sha256:7e9e2d068a107a2adafd19930011178d14a78326053cd4c3a9f7ad3fd049557d AS build
WORKDIR /src

# The dependencies build in a layer of their own, against a stub
# main.rs, so a cached build reuses them until a manifest or Cargo.lock
# changes. The registry and target directory stay in the layer, not in
# cache mounts, so the build below finds them unchanged.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY gateway/Cargo.toml gateway/
# Cargo loads every member of the workspace, so the server and
# test-issuer need their manifests and a target. The image never builds
# them, so an empty main and an empty library stand in for their source,
# which stays out of the build context.
COPY server/Cargo.toml server/
COPY test-issuer/Cargo.toml test-issuer/
RUN mkdir gateway/src server/src test-issuer/src \
    && echo 'fn main() {}' > gateway/src/main.rs \
    && echo 'fn main() {}' > server/src/main.rs \
    && touch test-issuer/src/lib.rs \
    && cargo build --locked --release --package transom-gateway \
      --target x86_64-unknown-linux-musl \
    && rm -r gateway/src

COPY . .
# COPY keeps the files' times from the checkout, which can be older than
# the stub's build, so touching main.rs makes cargo rebuild the gateway.
RUN touch gateway/src/main.rs \
    && cargo build --locked --release --package transom-gateway \
      --target x86_64-unknown-linux-musl \
    && cp target/x86_64-unknown-linux-musl/release/transom-gateway /transom-gateway

# The image holds the binary and the public CA certificates it needs to
# reach Google over TLS, and nothing else.
FROM scratch
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build /transom-gateway /transom-gateway
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/transom-gateway"]
