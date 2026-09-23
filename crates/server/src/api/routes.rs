use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{Html, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path as FsPath, PathBuf},
    sync::Arc,
};
use uuid::Uuid;
use tokio::io::AsyncWriteExt;
use crate::api::AppState;
use crate::core::traits::Job;
use crate::db::queries;

// ── Static UI ─────────────────────────────────────────────────────────────────

pub async fn root_handler() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

// ── Health ────────────────────────────────────────────────────────────────────

pub async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "cipherstream",
    })
}

// ── Upload registration ───────────────────────────────────────────────────────

/// Upload a complete local file and enqueue it for FFmpeg/HLS processing.
/// This is the primary beta path; resumable TUS registration remains separate.
pub async fn multipart_upload_handler(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, String)> {
    let mut saved: Option<(PathBuf, String, i64)> = None;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid multipart upload: {e}")))?
    {
        if field.name() != Some("file") {
            continue;
        }

        let video_id = new_video_id();
        let filename = sanitize_segment(field.file_name().unwrap_or("upload.bin"));
        let filename = if filename.is_empty() { "upload.bin".to_string() } else { filename };
        let path = state.upload_dir.join(format!("{video_id}-{filename}"));
        let mut output = tokio::fs::File::create(&path)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Could not create upload: {e}")))?;
        let mut size = 0_i64;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Could not read upload: {e}")))?
        {
            size += chunk.len() as i64;
            output
                .write_all(&chunk)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Could not save upload: {e}")))?;
        }
        saved = Some((path, filename, size));
        break;
    }

    let (input_file, filename, file_size) = saved
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Multipart field `file` is required.".to_string()))?;
    let video_id = new_video_id();

    queries::insert_video(
        &state.db, &video_id, &filename, Some(file_size), "local", None,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    queries::log_event(&state.db, &video_id, "upload_complete", Some(&filename)).await.ok();
    state.jobs.queued(&video_id).await;
    state.queue.publish(Job {
        id: video_id.clone(),
        input_file: input_file.to_string_lossy().into_owned(),
    }).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    queries::log_event(&state.db, &video_id, "queued", None).await.ok();

    Ok(Json(UploadResponse {
        video_id: video_id.clone(),
        status: "queued",
        manifest_url: format!("/stream/{video_id}/master.m3u8"),
        status_url: format!("/api/videos/{video_id}/status"),
    }))
}

pub async fn upload_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UploadRequest>,
) -> Result<Json<UploadResponse>, (StatusCode, String)> {
    let video_id = new_video_id();

    // Resolve input path from either a direct path or a TUS URL
    let input_file = payload
        .input_file
        .clone()
        .or_else(|| {
            payload
                .tus_url
                .as_deref()
                .and_then(|url| tus_url_to_local_path(url, &state.upload_dir))
        })
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "Provide `input_file` or a rustus `tus_url`.".to_string(),
            )
        })?;

    // Derive filename from the resolved path
    let filename = input_file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| video_id.clone());

    let tus_upload_id: Option<String> = payload
        .tus_url
        .as_deref()
        .and_then(|url| url.trim_end_matches('/').rsplit('/').next())
        .map(String::from);

    // Persist to SQLite
    queries::insert_video(
        &state.db,
        &video_id,
        &filename,
        payload.file_size,
        &state.storage_backend_label(),
        tus_upload_id.as_deref(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    queries::log_event(&state.db, &video_id, "upload_registered", Some(&filename))
        .await
        .ok();

    // Record any chunk info provided by the client
    if let Some(chunks) = payload.chunks {
        for (i, chunk) in chunks.iter().enumerate() {
            queries::record_chunk(
                &state.db,
                &video_id,
                tus_upload_id.as_deref().unwrap_or(""),
                i as i32,
                chunk.offset_bytes,
                chunk.size_bytes,
            )
            .await
            .ok();
        }
    }

    // Update in-memory status cache
    state.jobs.queued(&video_id).await;

    // Enqueue transcoding job
    let job = Job {
        id: video_id.clone(),
        input_file: input_file.to_string_lossy().into_owned(),
    };
    state
        .queue
        .publish(job)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    queries::log_event(&state.db, &video_id, "queued", None)
        .await
        .ok();

    Ok(Json(UploadResponse {
        video_id: video_id.clone(),
        status: "queued",
        manifest_url: format!("/stream/{video_id}/master.m3u8"),
        status_url: format!("/api/videos/{video_id}/status"),
    }))
}

// ── Video list ────────────────────────────────────────────────────────────────

pub async fn list_videos_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<queries::VideoRow>>, (StatusCode, String)> {
    queries::list_videos(&state.db)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

// ── Video detail ──────────────────────────────────────────────────────────────

pub async fn get_video_handler(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<VideoDetail>, (StatusCode, String)> {
    let video = queries::get_video(&state.db, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Video not found.".to_string()))?;

    let media_info = queries::get_media_info(&state.db, &id).await.ok().flatten();
    let renditions = queries::list_renditions(&state.db, &id)
        .await
        .unwrap_or_default();

    Ok(Json(VideoDetail {
        video,
        media_info,
        renditions,
    }))
}

// ── Video events ──────────────────────────────────────────────────────────────

pub async fn get_video_events_handler(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<queries::VideoEvent>>, (StatusCode, String)> {
    queries::list_events(&state.db, &id)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

// ── Video status (lightweight polling) ────────────────────────────────────────

pub async fn video_status_handler(
    Path(video_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::core::jobs::VideoStatus>, (StatusCode, String)> {
    // Try in-memory cache first (fastest), fall back to SQLite
    if let Some(status) = state.jobs.get(&video_id).await {
        return Ok(Json(status));
    }

    // Synthesise a status from the DB row if the process restarted
    let row = queries::get_video(&state.db, &video_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Video not found.".to_string()))?;

    use crate::core::jobs::{JobState, VideoStatus};
    let state_val = match row.status.as_str() {
        "queued" => JobState::Queued,
        "processing" => JobState::Processing,
        "ready" => JobState::Ready,
        _ => JobState::Failed,
    };

    Ok(Json(VideoStatus {
        video_id: row.id.clone(),
        state: state_val,
        playlist_url: row
            .manifest_url
            .unwrap_or_else(|| format!("/stream/{}/master.m3u8", row.id)),
        source_url: format!("/stream/{}/source.mp4", row.id),
        error: row.error,
    }))
}

// ── HLS streaming ─────────────────────────────────────────────────────────────

pub async fn stream_handler(
    Path((video_id, file)): Path<(String, String)>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, (StatusCode, String)> {
    let relative_file = sanitize_relative_path(&file)
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Invalid stream path.".to_string()))?;
    let path = state
        .hls_dir
        .join(sanitize_segment(&video_id))
        .join(relative_file);

    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "Stream file not found.".to_string()))?;

    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type_for(&path)),
    );
    // Allow browser HLS players to fetch cross-origin
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    Ok(response)
}

// ── Request / response types ──────────────────────────────────────────────────

/// Optional per-chunk info the frontend can include after a TUS upload.
#[derive(Deserialize)]
pub struct ChunkInfo {
    pub offset_bytes: i64,
    pub size_bytes: i64,
}

#[derive(Deserialize)]
pub struct UploadRequest {
    pub tus_url: Option<String>,
    pub input_file: Option<PathBuf>,
    /// Total file size in bytes (optional, supplied by client).
    pub file_size: Option<i64>,
    /// Optional chunk metadata array.
    pub chunks: Option<Vec<ChunkInfo>>,
}

#[derive(Serialize)]
pub struct UploadResponse {
    pub video_id: String,
    pub status: &'static str,
    pub manifest_url: String,
    pub status_url: String,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
}

#[derive(Serialize)]
pub struct VideoDetail {
    #[serde(flatten)]
    pub video: queries::VideoRow,
    pub media_info: Option<queries::VideoMediaInfo>,
    pub renditions: Vec<queries::VideoRendition>,
}

// ── AppState extension ────────────────────────────────────────────────────────

impl AppState {
    fn storage_backend_label(&self) -> String {
        // Peek at the storage Arc type name as a simple label
        // In practice this reads the config; we keep it simple here.
        "local".to_string()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn new_video_id() -> String {
    format!("video-{}", Uuid::new_v4().simple())
}

fn tus_url_to_local_path(url: &str, upload_dir: &FsPath) -> Option<PathBuf> {
    let upload_id = url.trim_end_matches('/').rsplit('/').next()?;
    Some(upload_dir.join(sanitize_segment(upload_id)))
}

fn sanitize_segment(segment: &str) -> String {
    segment
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect()
}

fn sanitize_relative_path(path: &str) -> Option<PathBuf> {
    let mut safe = PathBuf::new();
    for segment in path.split('/') {
        let clean = sanitize_segment(segment);
        if clean.is_empty() || clean != segment {
            return None;
        }
        safe.push(clean);
    }
    Some(safe)
}

fn content_type_for(path: &FsPath) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("m3u8") => "application/vnd.apple.mpegurl",
        Some("m4s") => "video/iso.segment",
        Some("ts") => "video/mp2t",
        Some("mp4") => "video/mp4",
        _ => "application/octet-stream",
    }
}
