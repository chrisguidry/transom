# syntax=docker/dockerfile:1

# transom has no C dependencies, so rust-lld links a fully static musl
# binary with nothing else installed.
FROM rust:1.99.1-alpine@sha256:7e9e2d068a107a2adafd19930011178d14a78326053cd4c3a9f7ad3fd049557d AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release --package transom \
      --target x86_64-unknown-linux-musl \
      --config "target.x86_64-unknown-linux-musl.linker='rust-lld'" \
    && cp target/x86_64-unknown-linux-musl/release/transom /transom

# The image holds the binary and nothing else.
FROM scratch
COPY --from=build /transom /transom
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/transom"]
CMD ["serve"]
