use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

// ── Domain types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct VideoRow {
    pub id: String,
    pub filename: String,
    pub file_size: Option<i64>,
    pub status: String,
    pub storage_backend: String,
    pub storage_key: Option<String>,
    pub manifest_url: Option<String>,
    pub tus_upload_id: Option<String>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct VideoMediaInfo {
    pub video_id: String,
    pub duration_secs: Option<f64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub fps: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i32>,
    pub bitrate_kbps: Option<i32>,
    pub format: Option<String>,
    pub probed_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct VideoRendition {
    pub id: i64,
    pub video_id: String,
    pub label: String,
    pub width: i32,
    pub height: i32,
    pub bitrate_kbps: i32,
    pub video_codec: String,
    pub audio_codec: String,
    pub playlist_url: Option<String>,
    pub segment_count: Option<i32>,
    pub status: String,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct UploadChunk {
    pub id: i64,
    pub video_id: String,
    pub tus_upload_id: String,
    pub chunk_index: i32,
    pub offset_bytes: i64,
    pub size_bytes: i64,
    pub received_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct VideoEvent {
    pub id: i64,
    pub video_id: String,
    pub event_type: String,
    pub message: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Aggregated analytics returned by GET /api/analytics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Analytics {
    pub total_videos: i64,
    pub total_storage_bytes: i64,
    pub total_chunks: i64,
    pub status_counts: StatusCounts,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct StatusCounts {
    pub uploading: i64,
    pub queued: i64,
    pub processing: i64,
    pub ready: i64,
    pub failed: i64,
}

// ── Videos ────────────────────────────────────────────────────────────────────

pub async fn insert_video(
    pool: &PgPool,
    id: &str,
    filename: &str,
    file_size: Option<i64>,
    storage_backend: &str,
    tus_upload_id: Option<&str>,
) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO videos (id, filename, file_size, status, storage_backend, tus_upload_id)
        VALUES ($1, $2, $3, 'queued', $4, $5)
        "#,
    )
    .bind(id)
    .bind(filename)
    .bind(file_size)
    .bind(storage_backend)
    .bind(tus_upload_id)
    .execute(pool)
    .await
    .map_err(|e| format!("insert_video: {e}"))?;
    Ok(())
}

pub async fn update_video_status(
    pool: &PgPool,
    id: &str,
    status: &str,
    error: Option<&str>,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE videos SET status = $1, error = $2, updated_at = NOW() WHERE id = $3",
    )
    .bind(status)
    .bind(error)
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| format!("update_video_status: {e}"))?;
    Ok(())
}

pub async fn update_video_manifest(
    pool: &PgPool,
    id: &str,
    manifest_url: &str,
    storage_key: Option<&str>,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE videos SET manifest_url = $1, storage_key = $2, status = 'ready', updated_at = NOW() WHERE id = $3",
    )
    .bind(manifest_url)
    .bind(storage_key)
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| format!("update_video_manifest: {e}"))?;
    Ok(())
}

pub async fn list_videos(pool: &PgPool) -> Result<Vec<VideoRow>, String> {
    sqlx::query_as::<_, VideoRow>(
        "SELECT * FROM videos ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("list_videos: {e}"))
}

pub async fn get_video(pool: &PgPool, id: &str) -> Result<Option<VideoRow>, String> {
    sqlx::query_as::<_, VideoRow>("SELECT * FROM videos WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("get_video: {e}"))
}

// ── Analytics ─────────────────────────────────────────────────────────────────

pub async fn get_analytics(pool: &PgPool) -> Result<Analytics, String> {
    let row: (i64, Option<i64>, i64) = sqlx::query_as(
        "SELECT COUNT(*), SUM(file_size), (SELECT COUNT(*) FROM upload_chunks) FROM videos",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| format!("get_analytics total: {e}"))?;

    let status_counts = sqlx::query_as::<_, (String, i64)>(
        "SELECT status, COUNT(*) FROM videos GROUP BY status",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("get_analytics status: {e}"))?;

    let mut counts = StatusCounts {
        uploading: 0, queued: 0, processing: 0, ready: 0, failed: 0,
    };
    for (status, n) in status_counts {
        match status.as_str() {
            "uploading"  => counts.uploading  = n,
            "queued"     => counts.queued     = n,
            "processing" => counts.processing = n,
            "ready"      => counts.ready      = n,
            "failed"     => counts.failed     = n,
            _ => {}
        }
    }

    Ok(Analytics {
        total_videos: row.0,
        total_storage_bytes: row.1.unwrap_or(0),
        total_chunks: row.2,
        status_counts: counts,
    })
}

// ── Media info ────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn upsert_media_info(
    pool: &PgPool,
    video_id: &str,
    duration_secs: Option<f64>,
    width: Option<i32>,
    height: Option<i32>,
    fps: Option<f64>,
    video_codec: Option<&str>,
    audio_codec: Option<&str>,
    audio_channels: Option<i32>,
    bitrate_kbps: Option<i32>,
    format: Option<&str>,
) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO video_media_info
            (video_id, duration_secs, width, height, fps, video_codec, audio_codec,
             audio_channels, bitrate_kbps, format)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
        ON CONFLICT (video_id) DO UPDATE SET
            duration_secs  = EXCLUDED.duration_secs,
            width          = EXCLUDED.width,
            height         = EXCLUDED.height,
            fps            = EXCLUDED.fps,
            video_codec    = EXCLUDED.video_codec,
            audio_codec    = EXCLUDED.audio_codec,
            audio_channels = EXCLUDED.audio_channels,
            bitrate_kbps   = EXCLUDED.bitrate_kbps,
            format         = EXCLUDED.format,
            probed_at      = NOW()
        "#,
    )
    .bind(video_id)
    .bind(duration_secs)
    .bind(width)
    .bind(height)
    .bind(fps)
    .bind(video_codec)
    .bind(audio_codec)
    .bind(audio_channels)
    .bind(bitrate_kbps)
    .bind(format)
    .execute(pool)
    .await
    .map_err(|e| format!("upsert_media_info: {e}"))?;
    Ok(())
}

pub async fn get_media_info(pool: &PgPool, video_id: &str) -> Result<Option<VideoMediaInfo>, String> {
    sqlx::query_as::<_, VideoMediaInfo>(
        "SELECT * FROM video_media_info WHERE video_id = $1",
    )
    .bind(video_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("get_media_info: {e}"))
}

// ── Renditions ────────────────────────────────────────────────────────────────

pub async fn insert_rendition(
    pool: &PgPool,
    video_id: &str,
    label: &str,
    width: i32,
    height: i32,
    bitrate_kbps: i32,
) -> Result<i64, String> {
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO video_renditions (video_id, label, width, height, bitrate_kbps)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(video_id)
    .bind(label)
    .bind(width)
    .bind(height)
    .bind(bitrate_kbps)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("insert_rendition: {e}"))?;
    Ok(row.0)
}

pub async fn mark_rendition_done(
    pool: &PgPool,
    rendition_id: i64,
    playlist_url: &str,
    segment_count: i32,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE video_renditions SET status='done', playlist_url=$1, segment_count=$2 WHERE id=$3",
    )
    .bind(playlist_url)
    .bind(segment_count)
    .bind(rendition_id)
    .execute(pool)
    .await
    .map_err(|e| format!("mark_rendition_done: {e}"))?;
    Ok(())
}

pub async fn mark_rendition_failed(
    pool: &PgPool,
    rendition_id: i64,
    error: &str,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE video_renditions SET status='failed', error=$1 WHERE id=$2",
    )
    .bind(error)
    .bind(rendition_id)
    .execute(pool)
    .await
    .map_err(|e| format!("mark_rendition_failed: {e}"))?;
    Ok(())
}

pub async fn list_renditions(pool: &PgPool, video_id: &str) -> Result<Vec<VideoRendition>, String> {
    sqlx::query_as::<_, VideoRendition>(
        "SELECT * FROM video_renditions WHERE video_id = $1 ORDER BY height ASC",
    )
    .bind(video_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("list_renditions: {e}"))
}

// ── Upload chunks ─────────────────────────────────────────────────────────────

pub async fn record_chunk(
    pool: &PgPool,
    video_id: &str,
    tus_upload_id: &str,
    chunk_index: i32,
    offset_bytes: i64,
    size_bytes: i64,
) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO upload_chunks (video_id, tus_upload_id, chunk_index, offset_bytes, size_bytes)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(video_id)
    .bind(tus_upload_id)
    .bind(chunk_index)
    .bind(offset_bytes)
    .bind(size_bytes)
    .execute(pool)
    .await
    .map_err(|e| format!("record_chunk: {e}"))?;
    Ok(())
}

pub async fn list_chunks(pool: &PgPool, video_id: &str) -> Result<Vec<UploadChunk>, String> {
    sqlx::query_as::<_, UploadChunk>(
        "SELECT * FROM upload_chunks WHERE video_id = $1 ORDER BY chunk_index ASC",
    )
    .bind(video_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("list_chunks: {e}"))
}

// ── Events ────────────────────────────────────────────────────────────────────

pub async fn log_event(
    pool: &PgPool,
    video_id: &str,
    event_type: &str,
    message: Option<&str>,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO video_events (video_id, event_type, message) VALUES ($1, $2, $3)",
    )
    .bind(video_id)
    .bind(event_type)
    .bind(message)
    .execute(pool)
    .await
    .map_err(|e| format!("log_event: {e}"))?;
    Ok(())
}

pub async fn list_events(pool: &PgPool, video_id: &str) -> Result<Vec<VideoEvent>, String> {
    sqlx::query_as::<_, VideoEvent>(
        "SELECT * FROM video_events WHERE video_id = $1 ORDER BY id ASC",
    )
    .bind(video_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("list_events: {e}"))
}
