# syntax=docker/dockerfile:1

# On Alpine the host is already musl, and rustc links musl binaries
# statically by default.
FROM rust:1.99.1-alpine@sha256:7e9e2d068a107a2adafd19930011178d14a78326053cd4c3a9f7ad3fd049557d AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release --package transom \
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
