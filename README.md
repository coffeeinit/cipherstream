# CipherStream

**A small Go app to upload, convert, stream, and share video.**

CipherStream is a single-process streaming demo: upload one or more videos in a browser, convert them to HLS, store the originals and output locally, and share a watch-page URL. Go owns the HTTP server, pages, upload handling, job queue, metadata, and stream delivery. FFmpeg runs privately on the server as the media engine; users do not configure or interact with it.

## What it does

- Server-rendered upload and watch pages; no frontend framework or JavaScript app.
- Multi-file uploads, with two conversions running at once.
- HLS output using H.264 video and AAC audio; FFmpeg decides which input formats it can read.
- Automatic stream-copy packaging when the input already has H.264 video and AAC audio.
- A thumbnail captured around 25% into each video.
- Conversion status and percentage shown on pages that refresh while work is running.
- A shareable `/watch/<id>` page and a direct HLS playlist URL.
- Local storage under `data/` (uploads, HLS segments, thumbnails, and small JSON metadata files).

## Run with Docker (recommended)

Docker builds the Go app and includes FFmpeg in the server image:

```bash
docker compose up --build
```

Open <http://localhost:8080>. To change the per-file upload ceiling, set `MAX_UPLOAD_BYTES` in your environment before starting Compose. The default is 4 GiB.

## Run directly

Requirements: Go 1.22+ and FFmpeg with `ffmpeg` and `ffprobe` available on the server's `PATH`.

```bash
go run .
```

Then open <http://localhost:8080>. The Go process invokes FFmpeg on the server; it is not a separate UI or service. FFmpeg support varies by build, so accepted media formats and hardware encoders depend on the FFmpeg build installed on the host.

## Settings

| Environment variable | Default | Purpose |
|---|---:|---|
| `PORT` | `8080` | HTTP port to listen on (use `:8080` or `0.0.0.0:8080` for an explicit address). |
| `DATA_DIR` | `data` | Root folder for originals, HLS output, and metadata. |
| `MAX_UPLOAD_BYTES` | `4294967296` | Maximum request size and per-file size in bytes; multipart batches share the request ceiling. |
| `FFMPEG_BIN` | `ffmpeg` | FFmpeg executable path. `ffprobe` is resolved beside it when a path is provided. |
| `FFMPEG_VIDEO_ENCODER` | `libx264` | FFmpeg video encoder used for conversion; set to an installed hardware encoder such as `h264_nvenc` only when that encoder is supported by the host. |

The upload form offers CRF 18–32 (default 23). Already-compatible H.264/AAC inputs are packaged without re-encoding, so CRF does not affect those files.

## Links and playback

- Home/library: `/`
- Share page: `/watch/<video-id>`
- Direct playlist: `/stream/<video-id>/index.m3u8`

The watch page uses the browser's native HLS support. Native HLS playback is available in browsers such as Safari; other browsers may need an HLS-compatible player. The playlist URL can also be opened in an HLS-capable player.

## Storage layout

```text
data/
  uploads/<id>.<source-extension>
  videos/<id>/video.json
  videos/<id>/index.m3u8
  videos/<id>/segment_00000.ts
  videos/<id>/thumbnail.jpg
```

Docker Compose preserves this directory in the `cipherstream-data` volume.

## Demo note

This is intentionally a small demo and has **no authentication**. Anyone who can reach the server can upload videos and view its library. Keep it on a trusted network unless you add access control, rate limits, and a storage-retention policy. Public sharing links are unlisted IDs, not access-controlled links.

## License

MIT. See [LICENSE](LICENSE).
