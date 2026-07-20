FROM rust:1-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release -p rustus -p cipherstream-server

FROM debian:bookworm-slim

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/cipherstream-server /usr/local/bin/cipherstream-server
COPY --from=builder /app/target/release/rustus /usr/local/bin/rustus
COPY config.example.toml /app/config.toml
RUN sed -i 's#binary_path = "./target/release/rustus"#binary_path = "/usr/local/bin/rustus"#' /app/config.toml

ENV CIPHERSTREAM_CONFIG=/app/config.toml
EXPOSE 8080 1081
CMD ["cipherstream-server"]
