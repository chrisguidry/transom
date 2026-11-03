# syntax=docker/dockerfile:1

# The builder runs on the build machine's own architecture and
# cross-compiles, which is much faster than building arm64 under emulation.
# transom has no C dependencies, so rust-lld links a fully static musl
# binary for either target without a cross C toolchain.
FROM --platform=$BUILDPLATFORM rust:1.99.1-alpine@sha256:7e9e2d068a107a2adafd19930011178d14a78326053cd4c3a9f7ad3fd049557d AS build
ARG TARGETARCH
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    case "$TARGETARCH" in \
      amd64) arch=x86_64 ;; \
      arm64) arch=aarch64 ;; \
      *) echo "unsupported architecture: $TARGETARCH" >&2; exit 1 ;; \
    esac \
    && target="$arch-unknown-linux-musl" \
    && cargo build --locked --release --package transom --target "$target" \
         --config "target.$target.linker='rust-lld'" \
    && cp "target/$target/release/transom" /transom

# The image holds the binary and nothing else.
FROM scratch
COPY --from=build /transom /transom
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/transom"]
CMD ["serve"]
