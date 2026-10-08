# CipherStream

**An importable Go package for upload → HLS conversion → streaming.**

CipherStream is a small Go video service that accepts a video reader, stores the original, converts it to HLS with server-side FFmpeg, tracks conversion status, and serves the playlist and segments. The package has no third-party Go dependencies. FFmpeg and ffprobe must be installed on the server.

## Use as a Go package

```bash
go get github.com/coffeeinit/cipherstream
```

```go
package main

import (
	"context"
	"fmt"
	"log"
	"net/http"
	"os"

	"github.com/coffeeinit/cipherstream"
)

func main() {
	service, err := cipherstream.New(cipherstream.Config{
		DataDir: "./video-data",
		Workers: 2,
	})
	if err != nil {
		log.Fatal(err)
	}
	defer service.Close()

	file, err := os.Open("input.mov")
	if err != nil {
		log.Fatal(err)
	}
	defer file.Close()

	video, err := service.Upload(context.Background(), "input.mov", file, 23)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println("video ID:", video.ID)
	fmt.Println("HLS playlist:", cipherstream.PlaylistURL(video.ID))

	mux := http.NewServeMux()
	mux.Handle("GET /hls/", http.StripPrefix("/hls/", service.HLSHandler()))
	log.Fatal(http.ListenAndServe(":8080", mux))
}
```

`Upload` returns queued metadata. Poll with `video, found := service.GetVideo(id)` until `found && video.Status == "ready"`, then use `PlaylistURL(id)` or mount `HLSHandler()` under the route your app needs. `service.Handler()` provides the complete demo UI, JSON API, watch pages, and HLS routes. Call `Close()` during shutdown to stop workers.

### Public package API

- `New(Config) (*Server, error)` — create storage and background conversion workers.
- `(*Server).Upload(ctx, filename, reader, crf)` — store and queue an upload; CRF 0 selects the default 23, otherwise use 18–32.
- `(*Server).GetVideo(id)` / `(*Server).Videos()` — read conversion status and metadata.
- `(*Server).HLSHandler()` — mount HLS playlists, segments, and thumbnails on your own Go router.
- `(*Server).Handler()` — run the included demo interface and HTTP API.
- `PlaylistURL(id)` / `WatchURL(id)` — get same-origin demo paths.

### Optional HTTP API

The included handler exposes `POST /api/videos?crf=23` with a multipart `file` field, `GET /api/videos`, and `GET /api/videos/{id}` for status polling. Responses include the HLS playlist and watch-page paths. The browser demo accepts multiple files at `/upload`.

## HLS playback

The demo watch page uses the locally bundled [hls.js](https://github.com/video-dev/hls.js/) HLS client with custom controls. It loads the generated `.m3u8` playlist only; it does not play the uploaded source file or fall back to a browser-native HLS player. HLS.js uses browser MediaSource/video rendering support, so playback is unavailable in browsers without that support. The generated playlist can also be used by any HLS-compatible player.

## Run the demo app

Requirements: Go 1.22+ and FFmpeg with `ffmpeg` and `ffprobe` available on the server's `PATH`.

```bash
go run ./cmd/cipherstream
```

Open <http://localhost:8080>. The upload form accepts multiple files without restricting extensions, shows queued/processing status, and offers CRF 18–32. FFmpeg probes each file: compatible H.264/AAC inputs are packaged without re-encoding; other readable video inputs are converted to H.264/AAC HLS. Accepted formats depend on the server's FFmpeg build, so unsupported, corrupt, audio-only, or DRM-protected files cannot be guaranteed.

Or use Docker Compose, which installs FFmpeg in the server image and persists output:

```bash
docker compose up --build
```

Open <http://localhost:8080>. The default per-file upload limit is 4 GiB; set `MAX_UPLOAD_BYTES` to change it.

## Configuration

`cipherstream.Config` accepts `DataDir`, `FFmpegPath`, `FFprobePath`, `VideoEncoder`, `MaxUploadBytes`, and `Workers`. The demo command also reads these environment variables:

| Variable | Default | Purpose |
|---|---:|---|
| `PORT` | `8080` | HTTP listen port/address. |
| `DATA_DIR` | `data` | Root for original uploads, HLS output, and metadata. |
| `MAX_UPLOAD_BYTES` | `4294967296` | Per-file upload limit; multipart batches share the same request ceiling. |
| `FFMPEG_BIN` | `ffmpeg` | Server-side FFmpeg executable. |
| `FFPROBE_BIN` | sibling `ffprobe` or PATH | Server-side ffprobe executable. |
| `FFMPEG_VIDEO_ENCODER` | `libx264` | Encoder for inputs that need transcoding; the selected encoder must be present in FFmpeg. |

## Storage layout

```text
data/
  videos/<id>/
    <id>.<source-extension>   # original upload
    video.json                # status and metadata
    index.m3u8                # HLS playlist
    segment_00000.ts          # HLS media segments
    thumbnail.jpg
```

Each video's source and HLS output are in the same directory. On startup, existing demo uploads found under the older `data/uploads/` layout are moved into their matching `data/videos/<id>/` folder.

This is a demo package, not a hosted SaaS: it has no authentication, retention policy, or access-control layer. Add those in the application that embeds it before exposing uploads or share links publicly.

## License

CipherStream is MIT licensed; see [LICENSE](LICENSE). The bundled HLS.js asset is Apache-2.0 licensed; see [web/HLSJS-LICENSE](web/HLSJS-LICENSE).
