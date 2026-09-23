# 🛡️ CipherStream

<div align="center">

**One application – upload, transcode, package, and stream video.  
Built to be extended, not rebuilt.**

</div>

CipherStream is a **local-first video engine** for uploading a file, transcoding it with FFmpeg, packaging adaptive HLS, and serving it to a browser. The beta keeps the path deliberately small and reliable: SQLite metadata, local filesystem storage, one in-process job queue, and a built-in web UI. S3 storage and resumable uploads are planned follow-up backends.

---

## ✨ What makes it special

- **One codebase, one binary** – upload, queue, transcode, and stream run in one server process.
- **Local by default** – SQLite metadata and local disk require no database or object-storage service.
- **FFmpeg HLS** – produces 360p, 720p, and 1080p variants plus a master playlist.
- **Built-in web UI** – choose a video file, monitor the job, and play the resulting HLS stream.

---

## 🚀 Core Features (beta)

| Feature | What it does |
|---------|--------------|
| **HTTP upload** | Upload a complete local video file with `POST /upload`. |
| **Transcoding** | FFmpeg creates configured HLS renditions in a background worker. |
| **HLS packaging** | Generates a master playlist and one variant playlist per rendition. |
| **Local storage** | Stores uploads, HLS output, and SQLite metadata under `data/`. |
| **Job status** | Poll `GET /api/videos/:id/status` while a job is queued or processing. |
| **Web UI** | Select a file, monitor transcoding, and play the HLS master playlist. |

---

## 🧩 Pluggable – extend without rewriting

We believe in **convention over configuration, but extension over modification**.  
The core engine is built around simple interfaces:

```rust
// Storage
#[async_trait]
pub trait Storage: Send + Sync {
    async fn put(&self, key: &str, data: &mut dyn AsyncRead) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Box<dyn AsyncRead>>;
    async fn signed_url(&self, key: &str, ttl: Duration) -> Result<String>;
}

// Queue
#[async_trait]
pub trait Queue: Send + Sync {
    async fn publish(&self, job: Job) -> Result<()>;
    async fn consume(&self) -> Result<Job>;
}

// Transcoder
#[async_trait]
pub trait Transcoder: Send + Sync {
    async fn transcode(&self, job: Job) -> Result<()>;
}

// Auth
#[async_trait]
pub trait Auth: Send + Sync {
    async fn authorize(&self, user_id: &str, video_id: &str) -> bool;
}
```

**Drop in your own implementations** for:

- Custom storage (Azure, Google Cloud, local with encryption)
- Different queue backends (RabbitMQ, Kafka, NATS)
- Alternative encoders (pure‑Rust `rav1e`, hardware‑specific)
- Custom filters (watermark, logo, overlay, AI upscaling)
- External DRM servers (Widevine, PlayReady)
- Your user database / authentication system

All through configuration or a few lines of code – no forking required.

---

## 🏗️ Architecture (the single‑app view)

```
┌─────────────────────────────────────────────────────────┐
│                     CipherStream                        │
│  ┌─────────────────────────────────────────────────┐   │
│  │           HTTP Router (REST + WebSocket)        │   │
│  └──────┬──────────────┬───────────────┬──────────┘   │
│         │              │               │              │
│  ┌──────▼─────┐ ┌──────▼─────┐ ┌──────▼─────┐      │
│  │  Upload    │ │  Transcode │ │  Streaming │      │
│  │  (TUS)     │ │  (parallel │ │  (signed   │      │
│  │            │ │   FFmpeg)  │ │   URLs)    │      │
│  └──────┬─────┘ └──────┬─────┘ └──────┬─────┘      │
│         │              │               │              │
│  ┌──────▼──────────────▼───────────────▼──────┐      │
│  │         Plug‑in interfaces                  │      │
│  │  (Storage, Queue, Transcoder, Auth, DRM)    │      │
│  └──────────────────────────────────────────────┘      │
│                         │                              │
│  ┌──────────────────────▼──────────────────────┐      │
│  │              Built‑in defaults               │      │
│  │   (local disk, in‑mem queue, FFmpeg,        │      │
│  │    JWT auth, signed URLs)                   │      │
│  └──────────────────────────────────────────────┘      │
└─────────────────────────────────────────────────────────┘
```

**All components live inside the same binary** – but they talk through interfaces, so you can replace any part without touching the rest.

---

## 🔌 How to add your own feature

1. **Identify the trait** you need to extend (see above).
2. **Write your implementation** in Rust.
3. **Register it** in the configuration file or via code.
4. **Restart** – your feature is now live.

**Example – adding a watermark filter**:

```rust
pub struct MyEncoder<E: Transcoder> {
    base: E,
}

#[async_trait]
impl<E: Transcoder> Transcoder for MyEncoder<E> {
    async fn transcode(&self, job: Job) -> Result<()> {
        // Add watermark using rust-ffmpeg filter graph
        // ...
        self.base.transcode(job).await
    }
}
```

Then register it: `config.Encoder = &MyEncoder{}`

That's it – no other changes needed.

---

## 🧪 Quick Start (single binary)

```bash
# 1. Clone the repository
git clone https://github.com/sudo-su-coffee/cipherstream.git
cd cipherstream

# 2. Install FFmpeg and build the server
# Ubuntu: sudo apt-get install ffmpeg
cargo build --release -p cipherstream-server

# 3. Run with local storage and SQLite
./target/release/cipherstream-server

# 4. Open http://localhost:8080 and select a video
```

The beta uses local storage by default. Edit `config.toml` only when you need to change the bind address,
data directory, rendition ladder, or FFmpeg path:

```toml
[database]
path = "data/cipherstream.db"

[storage]
type = "local"
data_dir = "data"

[transcoder]
ffmpeg_path = "/usr/bin/ffmpeg"
parallelism = 4
```

### Current runnable configuration

CipherStream uses TOML configuration. It loads `config.toml` by default, or the path in
`CIPHERSTREAM_CONFIG`.

```bash
cp config.example.toml config.toml
CIPHERSTREAM_CONFIG=config.toml cargo build --release -p cipherstream-server
CIPHERSTREAM_CONFIG=config.toml ./target/release/cipherstream-server
```

Docker Compose is also included for a local all-in-one run:

```bash
docker compose up --build
```

The Docker image bakes a container-safe `/app/config.toml`; for host runs, copy and edit
`config.example.toml`.

The local storage backend is wired first and stores uploads/HLS output under `data/`.
S3 storage, TUS resumable upload, authentication, and DRM are intentionally deferred until the
local upload-to-HLS path is stable.

---

## 📦 What's included (the complete package)

- **Backend** – Rust, with one HTTP server and one in-process background worker.
- **Frontend** – A small embedded browser UI with Vue and hls.js loaded from CDNs.
- **FFmpeg** – Uses `ffmpeg` and `ffprobe` installed on the host or in the Docker image.
- **Storage** – Local filesystem under `data/`.
- **Database** – SQLite under `data/cipherstream.db`.

---

## 📈 Scaling – from single instance to cluster

- **Single instance** – handles a moderate load (upload + transcode + stream).
- **Add workers** – run multiple instances of the same binary, point them to the same queue/storage, and they'll automatically distribute transcoding jobs.
- **Add CDN** – configure signed URLs to point to your CDN endpoint; segments are cached globally.

---

## 🗺️ Roadmap (already planned, but you can contribute)

- [x] Resumable upload (TUS)
- [x] Parallel chunked transcoding (FFmpeg)
- [x] HLS packaging
- [x] Signed‑URL delivery
- [x] Web UI (Vue + hls.js)
- [ ] DASH support
- [ ] GPU acceleration (NVENC, VAAPI)
- [ ] Real DRM (Widevine, PlayReady)
- [ ] Live streaming (RTMP → HLS)

---

## 🤝 Contributing – extend, improve, or plug in

We welcome contributions of all kinds – new plugins, bug fixes, documentation, or entire new features.  
Check out [CONTRIBUTING.md](CONTRIBUTING.md) and the [Plugin API](PLUGINS.md) guide to get started.

---

## 📄 License

[MIT](LICENSE) – use it freely, modify it, and make it your own.

---

**CipherStream – a video engine that grows with you.**
