use serde::Deserialize;
use std::{fs, net::SocketAddr, path::PathBuf};

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub queue: QueueConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub tus: TusConfig,
    #[serde(default)]
    pub transcoder: TranscoderConfig,
    #[serde(default)]
    pub signing: SigningConfig,
}

// ── Server ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: SocketAddr,
    /// Path to compiled Vue UI dist folder. Served as static files.
    #[serde(default = "default_ui_dist")]
    pub ui_dist: PathBuf,
}

// ── Database ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct DatabaseConfig {
    /// Full PostgreSQL connection URL.
    /// e.g. postgres://user:pass@localhost:5432/cipherstream
    #[serde(default = "default_db_url")]
    pub url: String,
    #[serde(default = "default_db_max_connections")]
    pub max_connections: u32,
}

// ── Queue ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct QueueConfig {
    /// "memory" (default, in-process) or "redis" (Redis Streams, persistent).
    #[serde(default = "default_queue_type")]
    pub r#type: String,
    /// Redis connection URL. Required when type = "redis".
    #[serde(default = "default_redis_url")]
    pub redis_url: String,
    /// Redis Streams key name.
    #[serde(default = "default_stream_key")]
    pub stream_key: String,
    /// Consumer group name (all workers in the same group share the load).
    #[serde(default = "default_consumer_group")]
    pub consumer_group: String,
}

// ── Storage ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_storage_type")]
    pub r#type: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    // S3-compatible
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
}

// ── TUS ───────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct TusConfig {
    #[serde(default = "default_tus_enabled")]
    pub enabled: bool,
    #[serde(default = "default_tus_host")]
    pub host: String,
    #[serde(default = "default_tus_port")]
    pub port: u16,
    pub binary_path: Option<PathBuf>,
}

// ── Transcoder ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct TranscoderConfig {
    #[serde(default = "default_parallelism")]
    pub parallelism: usize,
    pub ffmpeg_path: Option<PathBuf>,
    pub ffprobe_path: Option<PathBuf>,
    #[serde(default = "default_renditions")]
    pub renditions: Vec<RenditionConfig>,
}

/// A single output quality level in the HLS rendition ladder.
#[derive(Clone, Debug, Deserialize)]
pub struct RenditionConfig {
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub video_bitrate_kbps: u32,
    #[serde(default = "default_audio_bitrate")]
    pub audio_bitrate_kbps: u32,
}

// ── Signing ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
pub struct SigningConfig {
    #[serde(default = "default_url_ttl_seconds")]
    pub url_ttl_seconds: u64,
    pub base_url: Option<String>,
}

// ── impl Config ───────────────────────────────────────────────────────────────

impl Config {
    pub fn load() -> Result<Self, String> {
        let path = std::env::var("CIPHERSTREAM_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("config.toml"));

        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        toml::from_str(&contents)
            .map_err(|e| format!("Failed to parse {}: {e}", path.display()))
    }

    pub fn upload_dir(&self) -> PathBuf {
        self.storage.data_dir.join("uploads")
    }

    pub fn hls_dir(&self) -> PathBuf {
        self.storage.data_dir.join("hls")
    }

    pub fn ffmpeg_bin(&self) -> PathBuf {
        self.transcoder
            .ffmpeg_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("ffmpeg"))
    }

    pub fn ffprobe_bin(&self) -> PathBuf {
        self.transcoder
            .ffprobe_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("ffprobe"))
    }
}

// ── Default impls ─────────────────────────────────────────────────────────────

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
            queue: QueueConfig::default(),
            storage: StorageConfig::default(),
            tus: TusConfig::default(),
            transcoder: TranscoderConfig::default(),
            signing: SigningConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
            ui_dist: default_ui_dist(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: default_db_url(),
            max_connections: default_db_max_connections(),
        }
    }
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            r#type: default_queue_type(),
            redis_url: default_redis_url(),
            stream_key: default_stream_key(),
            consumer_group: default_consumer_group(),
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            r#type: default_storage_type(),
            data_dir: default_data_dir(),
            endpoint: None,
            bucket: None,
            region: None,
            access_key_id: None,
            secret_access_key: None,
        }
    }
}

impl Default for TusConfig {
    fn default() -> Self {
        Self {
            enabled: default_tus_enabled(),
            host: default_tus_host(),
            port: default_tus_port(),
            binary_path: None,
        }
    }
}

impl Default for TranscoderConfig {
    fn default() -> Self {
        Self {
            parallelism: default_parallelism(),
            ffmpeg_path: None,
            ffprobe_path: None,
            renditions: default_renditions(),
        }
    }
}

impl Default for SigningConfig {
    fn default() -> Self {
        Self {
            url_ttl_seconds: default_url_ttl_seconds(),
            base_url: None,
        }
    }
}

// ── default value fns ─────────────────────────────────────────────────────────

fn default_bind_addr() -> SocketAddr {
    "127.0.0.1:8080".parse().expect("default bind addr valid")
}
fn default_ui_dist() -> PathBuf { PathBuf::from("ui/dist") }
fn default_db_url() -> String {
    "postgres://cipherstream:secret@localhost:5432/cipherstream".to_string()
}
fn default_db_max_connections() -> u32 { 10 }
fn default_queue_type() -> String { "memory".to_string() }
fn default_redis_url() -> String { "redis://127.0.0.1:6379".to_string() }
fn default_stream_key() -> String { "cipherstream:jobs".to_string() }
fn default_consumer_group() -> String { "workers".to_string() }
fn default_storage_type() -> String { "local".to_string() }
fn default_data_dir() -> PathBuf { PathBuf::from("data") }
fn default_tus_enabled() -> bool { true }
fn default_tus_host() -> String { "0.0.0.0".to_string() }
fn default_tus_port() -> u16 { 1081 }
fn default_parallelism() -> usize { 4 }
fn default_audio_bitrate() -> u32 { 128 }
fn default_url_ttl_seconds() -> u64 { 3600 }

fn default_renditions() -> Vec<RenditionConfig> {
    vec![
        RenditionConfig { label: "360p".into(),  width: 640,  height: 360,  video_bitrate_kbps: 800,  audio_bitrate_kbps: 128 },
        RenditionConfig { label: "720p".into(),  width: 1280, height: 720,  video_bitrate_kbps: 2500, audio_bitrate_kbps: 128 },
        RenditionConfig { label: "1080p".into(), width: 1920, height: 1080, video_bitrate_kbps: 5000, audio_bitrate_kbps: 192 },
    ]
}
