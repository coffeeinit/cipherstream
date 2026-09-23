# 🛡️ CipherStream

<div align="center">

**One application – upload, transcode, package, and stream video.  
Built to be extended, not rebuilt.**

</div>

CipherStream is a **single, self‑contained video engine** that gives you a complete “YouTube‑in‑a‑box” experience – but it’s designed so you can **add your own features** without fighting the core. Drop in your own storage backend, plug in a custom encoder, add watermarking, or hook up your authentication system – all without touching the core pipeline.

---

## ✨ What makes it special

- **One codebase, one binary** – everything from upload to streaming runs in a single process (or can be split later if you outgrow it).
- **Pluggable by design** – every major component (storage, queue, encoder, DRM, auth) is an interface you can implement and swap in.
- **Fast by default** – parallel chunked transcoding, HLS adaptive streaming, and signed‑URL delivery for zero‑copy playback.
- **Honest security** – short‑lived signed URLs protect your content; real DRM (Widevine/PlayReady) is a configuration away when you need it.
- **Built‑in Vue UI** – a modern dashboard for uploads, job monitoring, and player – fully customizable.

---

## 🚀 Core Features (all included)

| Feature | What it does |
|---------|--------------|
| **Resumable uploads** | TUS‑compliant chunked uploads – resume after network failure. |
| **Parallel transcoding** | Split video at keyframes, encode all bitrates in parallel, stitch – uses your CPU cores efficiently. |
| **HLS packaging** | Generates adaptive HLS (fMP4/TS) with master & variant playlists. |
| **Object storage** | S3‑compatible (MinIO, AWS, Garage) – but you can plug in local disk or any other backend. |
| **Secure streaming** | Signed URLs with short expiry – no one can hotlink or download without permission. |
| **Job queue** | Built‑in in‑memory queue; swap with Redis for distributed workers. |
| **Web UI** | Vue 3 + hls.js – upload, watch, manage videos. |

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

# 2. Build from source (compiles native rust-ffmpeg bindings)
cargo build --release

# 3. Run with default config (local storage, embedded SQLite, native FFmpeg)
./target/release/cipherstream

# 3. Open http://localhost:8080 and start uploading
```

**Everything works out of the box** – but you can customize everything via `config.toml`:

```toml
[storage]
type = "s3"   # or "local"
endpoint = "http://minio:9000"
bucket = "videos"

[queue]
type = "redis"   # or "memory"
addr = "localhost:6379"

[auth]
type = "jwt"
secret = "your-secret"

[transcoder]
ffmpeg_path = "/usr/bin/ffmpeg"
parallelism = 4   # number of segments to encode concurrently
```

### Current runnable configuration

CipherStream uses TOML configuration. It loads `config.toml` by default, or the path in
`CIPHERSTREAM_CONFIG`.

```bash
cp config.example.toml config.toml
cargo build --release -p rustus -p cipherstream-server
CIPHERSTREAM_CONFIG=config.toml ./target/release/cipherstream-server
```

Docker Compose is also included for a local all-in-one run:

```bash
docker compose up --build
```

The Docker image bakes a container-safe `/app/config.toml`; for host runs, copy and edit
`config.example.toml`.

The local storage backend is wired first and stores uploads/HLS output under `data/`.
The `s3` storage mode is represented in config and compose via MinIO, and is the next backend
implementation target.

---

## 📦 What's included (the complete package)

- **Backend** – Rust (blazing fast, memory-safe, single binary, zero runtime dependencies).
- **Frontend** – Vue.js UI (built into the binary as embedded static files via `include_bytes!`).
- **FFmpeg** – Bundled directly in the repository. Compiled statically via `rust-ffmpeg` (no external downloads or installations required!).
- **Storage** – Works with local disk out of the box; connect to S3 in 1 line.
- **Queue** – In‑memory for development; swap to Redis for production.
- **Database** – Embedded SQLite out of the box (zero-config, high performance); swap to PostgreSQL for clustered environments.

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
