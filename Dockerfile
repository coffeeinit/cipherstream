FROM golang:1.22-bookworm AS build
WORKDIR /src
COPY go.mod ./
COPY cipherstream.go ./
COPY web ./web
COPY cmd ./cmd
RUN CGO_ENABLED=0 go build -trimpath -ldflags="-s -w" -o /out/cipherstream ./cmd/cipherstream

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates ffmpeg \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /app cipherstream \
    && mkdir -p /data \
    && chown -R cipherstream:cipherstream /data
WORKDIR /app
COPY --from=build /out/cipherstream /usr/local/bin/cipherstream
ENV PORT=8080 DATA_DIR=/data
EXPOSE 8080
VOLUME ["/data"]
USER cipherstream
ENTRYPOINT ["cipherstream"]
