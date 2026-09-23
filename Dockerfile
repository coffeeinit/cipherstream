FROM rust:1.98-bookworm AS builder

WORKDIR /app
COPY . .
ENV CARGO_TARGET_DIR=/app/target
RUN cargo build --release -p cipherstream-server

FROM debian:bookworm-slim

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/cipherstream-server /usr/local/bin/cipherstream-server
COPY config.example.toml /app/config.toml
RUN sed -i 's#bind_addr = "127.0.0.1:8080"#bind_addr = "0.0.0.0:8080"#' /app/config.toml

ENV CIPHERSTREAM_CONFIG=/app/config.toml
EXPOSE 8080
CMD ["cipherstream-server"]
