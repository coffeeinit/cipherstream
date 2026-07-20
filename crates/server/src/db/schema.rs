use sqlx::PgPool;

/// Apply all DDL. Safe to re-run — all statements use IF NOT EXISTS / CREATE INDEX IF NOT EXISTS.
pub async fn apply(pool: &PgPool) -> Result<(), String> {
    // ── videos ────────────────────────────────────────────────────────────────
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS videos (
            id               TEXT        PRIMARY KEY,
            filename         TEXT        NOT NULL,
            file_size        BIGINT,
            status           TEXT        NOT NULL DEFAULT 'uploading',
            storage_backend  TEXT        NOT NULL DEFAULT 'local',
            storage_key      TEXT,
            manifest_url     TEXT,
            tus_upload_id    TEXT,
            error            TEXT,
            created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create videos table: {e}"))?;

    // ── video_media_info ──────────────────────────────────────────────────────
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS video_media_info (
            video_id        TEXT        PRIMARY KEY REFERENCES videos(id) ON DELETE CASCADE,
            duration_secs   DOUBLE PRECISION,
            width           INTEGER,
            height          INTEGER,
            fps             DOUBLE PRECISION,
            video_codec     TEXT,
            audio_codec     TEXT,
            audio_channels  INTEGER,
            bitrate_kbps    INTEGER,
            format          TEXT,
            probed_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create video_media_info table: {e}"))?;

    // ── video_renditions ──────────────────────────────────────────────────────
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS video_renditions (
            id              BIGSERIAL   PRIMARY KEY,
            video_id        TEXT        NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
            label           TEXT        NOT NULL,
            width           INTEGER     NOT NULL,
            height          INTEGER     NOT NULL,
            bitrate_kbps    INTEGER     NOT NULL,
            video_codec     TEXT        NOT NULL DEFAULT 'h264',
            audio_codec     TEXT        NOT NULL DEFAULT 'aac',
            playlist_url    TEXT,
            segment_count   INTEGER,
            status          TEXT        NOT NULL DEFAULT 'pending',
            error           TEXT,
            created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create video_renditions table: {e}"))?;

    // ── upload_chunks ─────────────────────────────────────────────────────────
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS upload_chunks (
            id              BIGSERIAL   PRIMARY KEY,
            video_id        TEXT        NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
            tus_upload_id   TEXT        NOT NULL,
            chunk_index     INTEGER     NOT NULL,
            offset_bytes    BIGINT      NOT NULL,
            size_bytes      BIGINT      NOT NULL,
            received_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create upload_chunks table: {e}"))?;

    // ── video_events ──────────────────────────────────────────────────────────
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS video_events (
            id          BIGSERIAL   PRIMARY KEY,
            video_id    TEXT        NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
            event_type  TEXT        NOT NULL,
            message     TEXT,
            created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create video_events table: {e}"))?;

    // ── indexes ───────────────────────────────────────────────────────────────
    for idx_sql in [
        "CREATE INDEX IF NOT EXISTS idx_videos_status       ON videos(status);",
        "CREATE INDEX IF NOT EXISTS idx_videos_created_at   ON videos(created_at DESC);",
        "CREATE INDEX IF NOT EXISTS idx_renditions_video_id ON video_renditions(video_id);",
        "CREATE INDEX IF NOT EXISTS idx_chunks_video_id     ON upload_chunks(video_id);",
        "CREATE INDEX IF NOT EXISTS idx_events_video_id     ON video_events(video_id);",
    ] {
        sqlx::query(idx_sql)
            .execute(pool)
            .await
            .map_err(|e| format!("Failed to create index: {e}"))?;
    }

    Ok(())
}
