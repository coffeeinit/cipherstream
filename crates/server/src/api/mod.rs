use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use crate::core::jobs::JobStatusStore;
use crate::core::traits::Queue;
use crate::core::traits::Storage;
use crate::db::SqlitePool;

pub struct AppState {
    pub queue: Arc<dyn Queue>,
    pub storage: Arc<dyn Storage>,
    pub jobs: JobStatusStore,
    pub upload_dir: std::path::PathBuf,
    pub hls_dir: std::path::PathBuf,
    /// Shared SQLite connection pool used by all handlers.
    pub db: Arc<SqlitePool>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // UI
        .route("/", get(routes::root_handler))
        // Health
        .route("/health", get(routes::health_handler))
        // Direct local upload; TUS registration remains available separately.
        .route("/upload", post(routes::multipart_upload_handler))
        .route("/api/uploads/complete", post(routes::upload_handler))
        // Video list + detail
        .route("/api/videos", get(routes::list_videos_handler))
        .route("/api/videos/:id", get(routes::get_video_handler))
        .route("/api/videos/:id/events", get(routes::get_video_events_handler))
        .route("/api/videos/:id/status", get(routes::video_status_handler))
        // Backwards-compat status path (no /api/ prefix)
        .route("/videos/:video_id/status", get(routes::video_status_handler))
        // HLS streaming
        .route("/stream/:video_id/*file", get(routes::stream_handler))
        .with_state(state)
}

pub mod routes;
