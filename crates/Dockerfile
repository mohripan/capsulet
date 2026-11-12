# Every Rust component from one build. The `build` stage compiles all five
# binaries in a single cargo invocation; each component is a target that puts
# its binary on the same runtime base:
#
#   docker build -f crates/Dockerfile --target api --build-arg VERSION=... .
#
# with REVISION, CREATED and SOURCE passed the same way.
#
# Targets built in one BuildKit session, as Compose and the verifier's
# `containers` gate do, share the build stage, so the workspace compiles once.
# `capsulet-xtask release images --plan` prints the image name, target and
# build arguments of each component from release.toml.

FROM rust:1.99-bookworm@sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c AS build
ENV RUSTUP_TOOLCHAIN=1.96.0
ENV CARGO_HTTP_MULTIPLEXING=false
ENV CARGO_HTTP_TIMEOUT=600
ENV CARGO_NET_RETRY=10

WORKDIR /app
COPY . .
RUN --mount=type=cache,id=capsulet-cargo-registry-v3,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=capsulet-target-v2,target=/app/target,sharing=locked \
    cargo build --release --locked \
        --bin capsulet-api \
        --bin capsulet-worker \
        --bin capsulet-scheduler \
        --bin capsulet-graph-worker \
        --bin capsulet-evaluator \
    && mkdir -p /out \
    && cp target/release/capsulet-api target/release/capsulet-worker \
        target/release/capsulet-scheduler target/release/capsulet-graph-worker \
        target/release/capsulet-evaluator /out/

FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc AS runtime

# `upgrade` before `install`, so a rebuild picks up fixes Debian published
# after this tag was cut. Without it only the named packages are current while
# the rest of the base image stays frozen, which is how an image accumulates
# fixable advisories although its Dockerfile never changed.
RUN apt-get update \
    && apt-get upgrade -y \
    && apt-get install -y --no-install-recommends ca-certificates curl python3 python3-pip \
    && groupadd --gid 10001 capsulet \
    && useradd --uid 10001 --gid 10001 --no-create-home --shell /usr/sbin/nologin capsulet \
    && mkdir -p /capsulet/artifacts /capsulet/objects \
    && chown -R 10001:10001 /capsulet \
    && rm -rf /var/lib/apt/lists/*

# Declared after the packages, so a new revision does not reinstall them.
ARG VERSION
ARG REVISION
ARG CREATED
ARG SOURCE
LABEL org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.created="${CREATED}" \
      org.opencontainers.image.source="${SOURCE}" \
      org.opencontainers.image.url="${SOURCE}" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.vendor="Capsulet"

USER 10001:10001

FROM runtime AS api
LABEL org.opencontainers.image.title="capsulet-api" \
      org.opencontainers.image.description="Capsulet HTTP control plane"
COPY --from=build /out/capsulet-api /usr/local/bin/capsulet-api
ENTRYPOINT ["/usr/local/bin/capsulet-api"]

FROM runtime AS worker
LABEL org.opencontainers.image.title="capsulet-worker" \
      org.opencontainers.image.description="Capsulet job worker"
COPY --from=build /out/capsulet-worker /usr/local/bin/capsulet-worker
ENTRYPOINT ["/usr/local/bin/capsulet-worker"]

FROM runtime AS scheduler
LABEL org.opencontainers.image.title="capsulet-scheduler" \
      org.opencontainers.image.description="Capsulet run scheduler"
COPY --from=build /out/capsulet-scheduler /usr/local/bin/capsulet-scheduler
ENTRYPOINT ["/usr/local/bin/capsulet-scheduler"]

FROM runtime AS graph-worker
LABEL org.opencontainers.image.title="capsulet-graph-worker" \
      org.opencontainers.image.description="Capsulet verified-computation graph worker"
COPY --from=build /out/capsulet-graph-worker /usr/local/bin/capsulet-graph-worker
ENTRYPOINT ["/usr/local/bin/capsulet-graph-worker"]

FROM runtime AS evaluator
LABEL org.opencontainers.image.title="capsulet-evaluator" \
      org.opencontainers.image.description="Capsulet trigger evaluator and retention"
COPY --from=build /out/capsulet-evaluator /usr/local/bin/capsulet-evaluator
ENTRYPOINT ["/usr/local/bin/capsulet-evaluator"]
