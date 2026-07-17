# 🛡️ CipherStream

<div align="center">
  <p><strong>A high-performance, self-hosted video processing and streaming engine.</strong></p>
</div>

CipherStream provides a "YouTube-in-a-box" experience designed for absolute maximum speed and simplicity. Built purely in Rust and Go, it drops traditional edge-auth proxies in favor of strict, native Digital Rights Management (DRM). 

Video files are served directly from public object storage to the client player. This ensures zero network bottlenecks, maximum latency reduction, and mathematically unbreakable piracy protection.

## 🚀 Core Features

* **Zero-Overhead Delivery:** Video chunks stream directly from the S3 storage node to the user's browser. No proxies, no middleware, no bandwidth limits.
* **Bulletproof DRM Security:** Powered by `oximedia-drm`, all chunks are AES-128 encrypted using Common Encryption (CENC). Even if a pirate downloads your raw video files, they cannot play them without a license key.
* **Resumable Uploads:** Powered by the Go-based TUS protocol (`tusd`). If the internet drops, 10GB+ video uploads resume exactly where they left off.
* **Pure-Rust Transcoding:** Utilizes the memory-safe `OxiMedia` framework. Automatically converts uploads into adaptive formats (1080p, 720p, 480p) and separates audio tracks without relying on unsafe C/FFmpeg wrappers.

---

## 🌊 Architecture Lifecycle

The entire ecosystem is reduced to three highly efficient, specialized nodes. 

### 1. Ingest Node (Go)
* Handles the raw uploads using the TUS protocol. 
* Accepts massive video files in small, resumable blocks.
* Upon completion, triggers a webhook to hand the file over to the Processing Node.

### 2. Processing & Packaging Node (Rust)
* **Demux & Transcode:** Uses the pure-Rust `oximedia` crate to demux the container, extract the audio, and transcode the video into multiple adaptive bitrates.
* **Package & Encrypt:** Uses `oximedia-packager` and `oximedia-drm` to slice the media into `.ts` or `fMP4` chunks. It encrypts every single chunk using AES-128 and generates the master `.m3u8` playlist.
* **Push to Vault:** Pushes the encrypted chunks directly to the Storage Vault.

### 3. Storage Vault (Rust/Go)
* Uses a highly resilient, self-hosted S3-compatible object store (e.g., Garage or SeaweedFS).
* Because the chunks are cryptographically locked by DRM, this storage bucket is configured as **Public**. It acts as a hyper-fast file server that pushes bytes to the internet as quickly as possible.

### 4. DRM License Server (Rust)
* Security is enforced exactly at the moment of playback.
* When a user hits "Play", the player requests a decryption key.
* This lightweight Rust API checks access policies and instantly issues the W3C Clear Key or Widevine/PlayReady keys to authorized viewers.

---

## 🗄️ Database Schema

CipherStream uses PostgreSQL to track metadata, storage distribution, and access control.

* `Videos`: Tracks `video_id`, title, total storage footprint, and a soft-delete toggle.
* `Streams`: Tracks the specific media types inside a video (e.g., `video_1080p`, `audio_en`).
* `Chunks`: Maps every `.ts` file to its physical S3 storage node, allowing admins to monitor disk usage.
* `Access_Policies`: Defines which users, roles, or email addresses are permitted to request a DRM decryption key for a specific `video_id`.

---

## 🛠️ Getting Started

### Prerequisites
* Rust 1.85+ (For OxiMedia compilation)
* Go 1.21+ (For TUSD Ingest)
* PostgreSQL 15+
* Garage or SeaweedFS cluster

### Quick Start (Development)

1. **Clone the repository**
   ```bash
   git clone [https://github.com/yourusername/CipherStream.git](https://github.com/yourusername/CipherStream.git)
   cd CipherStream
   
