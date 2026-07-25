# Multi-stage build for the act-web-server Rust binary (cargo workspace).
FROM rust:1-bookworm AS builder
WORKDIR /usr/src/app

# Fetch dependencies in a cacheable layer.
COPY Cargo.toml Cargo.lock ./
COPY migration/Cargo.toml ./migration/Cargo.toml
RUN mkdir -p src migration/src \
    && echo 'fn main() {}' > src/main.rs \
    && echo 'fn main() {}' > migration/src/main.rs \
    && echo '' > migration/src/lib.rs \
    && cargo fetch

# Build the real sources.
COPY . .
RUN cargo build --release --bin act_web_server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --no-create-home appuser
USER 10001

WORKDIR /app
COPY --from=builder /usr/src/app/target/release/act_web_server /usr/local/bin/act-web-server

ENV PORT=8080
EXPOSE 8080
ENTRYPOINT ["act-web-server"]
