FROM rust:1.96-bookworm AS build

ARG PACKAGE
ARG BIN

WORKDIR /app
COPY . .
RUN cargo build --release -p "${PACKAGE}" --bin "${BIN}"

FROM debian:bookworm-slim

ARG BIN
ENV CAPSULET_BIN="${BIN}"

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl python3 \
    && groupadd --gid 10001 capsulet \
    && useradd --uid 10001 --gid 10001 --no-create-home --shell /usr/sbin/nologin capsulet \
    && mkdir -p /capsulet/artifacts \
    && chown -R 10001:10001 /capsulet \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /app/target/release/${BIN} /usr/local/bin/${BIN}

USER 10001:10001

ENTRYPOINT ["/bin/sh", "-c", "exec /usr/local/bin/${CAPSULET_BIN}"]
