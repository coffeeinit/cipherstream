use sqlx::SqlitePool;

pub async fn apply(pool: &SqlitePool) -> Result<(), String> {
    for statement in [
        r#"CREATE TABLE IF NOT EXISTS videos (
            id TEXT PRIMARY KEY, filename TEXT NOT NULL, file_size INTEGER,
            status TEXT NOT NULL DEFAULT 'uploading', storage_backend TEXT NOT NULL DEFAULT 'local',
            storage_key TEXT, manifest_url TEXT, tus_upload_id TEXT, error TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS video_media_info (
            video_id TEXT PRIMARY KEY REFERENCES videos(id) ON DELETE CASCADE, duration_secs REAL,
            width INTEGER, height INTEGER, fps REAL, video_codec TEXT, audio_codec TEXT,
            audio_channels INTEGER, bitrate_kbps INTEGER, format TEXT,
            probed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS video_renditions (
            id INTEGER PRIMARY KEY AUTOINCREMENT, video_id TEXT NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
            label TEXT NOT NULL, width INTEGER NOT NULL, height INTEGER NOT NULL, bitrate_kbps INTEGER NOT NULL,
            video_codec TEXT NOT NULL DEFAULT 'h264', audio_codec TEXT NOT NULL DEFAULT 'aac',
            playlist_url TEXT, segment_count INTEGER, status TEXT NOT NULL DEFAULT 'pending', error TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS upload_chunks (
            id INTEGER PRIMARY KEY AUTOINCREMENT, video_id TEXT NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
            tus_upload_id TEXT NOT NULL, chunk_index INTEGER NOT NULL, offset_bytes INTEGER NOT NULL,
            size_bytes INTEGER NOT NULL, received_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        r#"CREATE TABLE IF NOT EXISTS video_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT, video_id TEXT NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
            event_type TEXT NOT NULL, message TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )"#,
        "CREATE INDEX IF NOT EXISTS idx_videos_status ON videos(status)",
        "CREATE INDEX IF NOT EXISTS idx_videos_created_at ON videos(created_at DESC)",
        "CREATE INDEX IF NOT EXISTS idx_renditions_video_id ON video_renditions(video_id)",
        "CREATE INDEX IF NOT EXISTS idx_chunks_video_id ON upload_chunks(video_id)",
        "CREATE INDEX IF NOT EXISTS idx_events_video_id ON video_events(video_id)",
    ] {
        sqlx::query(statement).execute(pool).await.map_err(|e| format!("Database schema error: {e}"))?;
    }
    Ok(())
}
