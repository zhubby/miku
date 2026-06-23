FROM rust:1.92-bookworm AS builder

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake git libssl-dev make pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock Makefile rust-toolchain.toml rustfmt.toml ./
RUN rustup target add wasm32-unknown-unknown

COPY crates ./crates

RUN make install-wasm-bindgen
RUN make build-web-assets
RUN cargo build --release -p miku-cli
RUN install -m 0755 target/release/miku /usr/local/bin/miku

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 miku \
    && useradd --system --uid 10001 --gid miku --home-dir /home/miku --create-home miku \
    && mkdir -p /data \
    && chown -R miku:miku /data /home/miku

COPY --from=builder /usr/local/bin/miku /usr/local/bin/miku

ENV HOME=/home/miku \
    MIKU_CONFIG_DIR=/data \
    MIKU_LOG_LEVEL=info

USER miku
EXPOSE 5174
VOLUME ["/data"]

ENTRYPOINT ["miku"]
CMD ["server", "--bind", "0.0.0.0:5174"]
