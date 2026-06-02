FROM rust:1.87-bookworm AS build

ARG PACKAGE
ARG BIN

WORKDIR /app
COPY . .
RUN cargo build --release -p "${PACKAGE}" --bin "${BIN}"

FROM debian:bookworm-slim

ARG BIN
ENV CAPSULET_BIN="${BIN}"

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /app/target/release/${BIN} /usr/local/bin/${BIN}

ENTRYPOINT ["/bin/sh", "-c", "exec /usr/local/bin/${CAPSULET_BIN}"]
