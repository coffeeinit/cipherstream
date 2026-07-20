# CipherStream Phase 1 PR Plan

## Summary
- Build a PR-ready v1 Rust streaming engine: chunked upload, SQLite metadata/logs, FFmpeg HLS `.m3u8`, local/S3 storage, stream serving, and Vue UI.
- Rust owns the engine: TUS upload intake, DB, job state, storage, HLS packaging, APIs, and streaming.
- Vue owns the operator/user UI: upload video, list videos, inspect metadata/logs, copy stream link, and play HLS.

## Key Changes
- **Upload + Processing**
  - Use existing Rust `rustus` TUS server for resumable chunk uploads.
  - Register completed uploads in CipherStream and enqueue an HLS packaging job.
  - Use FFmpeg CLI from Rust for Phase 1 HLS output: `master.m3u8` plus segments.
  - Track lifecycle states: `uploading`, `uploaded`, `queued`, `processing`, `ready`, `failed`.

- **SQLite Metadata + Logs**
  - Add SQLite as the Phase 1 database.
  - Store videos, upload metadata, storage backend/key/path, HLS manifest URL, status, errors, created/updated timestamps.
  - Store job/event logs for upload registration, queue transitions, FFmpeg command/result, storage writes, and failures.
  - Store enough chunk/upload information from TUS to show chunk-aware history where available.

- **Storage**
  - Implement storage abstraction with both working backends in Phase 1:
    - Local mapped folder storage using TOML `data_dir`.
    - S3-compatible storage using TOML `endpoint`, `bucket`, `region`, credentials from env/config.
  - Stream locally from disk for local backend.
  - For S3, store HLS assets in bucket and return signed or configured public stream URLs.

- **Vue UI**
  - Replace placeholder UI with upload, video list, metadata panel, event/log panel, and HLS player.
  - Show file name, video id, status, storage backend, stream URL, created time, updated time, and latest error.
  - Poll status after upload and refresh list automatically.
  - Use `hls.js` for `.m3u8` playback and expose copyable stream links.

- **Docs + Release**
  - Add `CHANGELOG.md` for `v1.0.0-phase1`.
  - Update README to honestly mark Phase 1 complete items only.
  - Keep TOML config as the only config format.
  - Update Docker Compose with app + optional MinIO for S3 testing.

## Public Interfaces
- `POST /api/uploads/complete` registers a completed TUS upload and queues processing.
- `GET /api/videos` lists videos with metadata and current status.
- `GET /api/videos/:id` returns full metadata.
- `GET /api/videos/:id/events` returns DB event/job logs.
- `GET /api/videos/:id/status` returns lightweight polling status.
- `GET /stream/:id/master.m3u8` serves or redirects to the HLS manifest.
- TOML config includes server, database, tus, storage, transcoder, and signing/URL settings.

## Test Plan
- Run `cargo check -p cipherstream-server`.
- Run `cargo fmt --check`.
- Start local mode with SQLite + local storage.
- Upload a video via Vue using TUS.
- Confirm SQLite rows exist for video, job, and events.
- Confirm FFmpeg creates `master.m3u8` and segments.
- Confirm Vue list, metadata, logs, link copy, and playback work.
- Start S3/MinIO mode and confirm HLS files are stored in bucket.
- Confirm failed FFmpeg/storage paths mark status `failed` and show error in Vue.

## Assumptions
- Phase 1 uses TUS chunk uploads through `rustus`, not custom chunk APIs.
- Phase 1 uses FFmpeg CLI orchestrated by Rust for reliable HLS.
- SQLite is the Phase 1 database; PostgreSQL is planned later.
- Local folder and S3-compatible storage both work in Phase 1.
- Auth, DRM, DASH, live RTMP, Redis queue, and PostgreSQL are out of Phase 1.
