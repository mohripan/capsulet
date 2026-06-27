FROM rust:1.96-bookworm AS build
ENV RUSTUP_TOOLCHAIN=1.96.0
ENV CARGO_HTTP_MULTIPLEXING=false
ENV CARGO_HTTP_TIMEOUT=600
ENV CARGO_NET_RETRY=10

ARG PACKAGE
ARG BIN

WORKDIR /app
COPY . .
RUN --mount=type=cache,id=capsulet-cargo-registry-v3,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=capsulet-target-v2,target=/app/target,sharing=locked \
    cargo build --release -p "${PACKAGE}" --bin "${BIN}" \
    && cp "/app/target/release/${BIN}" /tmp/capsulet-bin

FROM gcr.io/distroless/cc-debian12:nonroot

ARG BIN

COPY --from=build /tmp/capsulet-bin /usr/local/bin/capsulet-bin

ENTRYPOINT ["/usr/local/bin/capsulet-bin"]
