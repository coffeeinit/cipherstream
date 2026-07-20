use crate::{
    config::RenditionConfig,
    core::traits::{Job, Transcoder},
    db::{
        queries::{
            insert_rendition, log_event, mark_rendition_done, mark_rendition_failed,
            upsert_media_info,
        },
        SqlitePool,
    },
};
use async_trait::async_trait;
use serde::Deserialize;
use std::{path::Path, path::PathBuf, sync::Arc};
use tokio::process::Command;

// ── NativeFFmpegTranscoder ───────────────────────────────────────────────────

pub struct NativeFFmpegTranscoder {
    ffmpeg_bin: PathBuf,
    ffprobe_bin: PathBuf,
    renditions: Vec<RenditionConfig>,
    pool: Arc<SqlitePool>,
}

impl NativeFFmpegTranscoder {
    pub fn new(
        ffmpeg_bin: PathBuf,
        ffprobe_bin: PathBuf,
        renditions: Vec<RenditionConfig>,
        pool: Arc<SqlitePool>,
    ) -> Self {
        Self {
            ffmpeg_bin,
            ffprobe_bin,
            renditions,
            pool,
        }
    }
}

// ── ffprobe output schema (subset) ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
    bit_rate: Option<String>,
    format_name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    r_frame_rate: Option<String>,
    channels: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    format: Option<ProbeFormat>,
    #[serde(default)]
    streams: Vec<ProbeStream>,
}

// ── Transcoder impl ──────────────────────────────────────────────────────────

#[async_trait]
impl Transcoder for NativeFFmpegTranscoder {
    async fn transcode(&self, job: &Job, output_dir: &Path) -> Result<(), String> {
        let video_id = &job.id;
        let input = &job.input_file;

        // ── 1. ffprobe: inspect source ───────────────────────────────────────
        log_event(&self.pool, video_id, "ffprobe_start", None)
            .await
            .ok();

        let probe_result = run_ffprobe(&self.ffprobe_bin, input).await;
        match probe_result {
            Ok(info) => {
                let video_stream = info
                    .streams
                    .iter()
                    .find(|s| s.codec_type.as_deref() == Some("video"));
                let audio_stream = info
                    .streams
                    .iter()
                    .find(|s| s.codec_type.as_deref() == Some("audio"));

                let duration_secs = info
                    .format
                    .as_ref()
                    .and_then(|f| f.duration.as_deref())
                    .and_then(|d| d.parse::<f64>().ok());
                let bitrate_kbps = info
                    .format
                    .as_ref()
                    .and_then(|f| f.bit_rate.as_deref())
                    .and_then(|b| b.parse::<i64>().ok())
                    .map(|b| b / 1000);
                let format = info
                    .format
                    .as_ref()
                    .and_then(|f| f.format_name.clone());

                let width = video_stream.and_then(|s| s.width);
                let height = video_stream.and_then(|s| s.height);
                let fps = video_stream
                    .and_then(|s| s.r_frame_rate.as_deref())
                    .and_then(parse_fps);
                let video_codec = video_stream.and_then(|s| s.codec_name.clone());
                let audio_codec = audio_stream.and_then(|s| s.codec_name.clone());
                let audio_channels = audio_stream.and_then(|s| s.channels);

                upsert_media_info(
                    &self.pool,
                    video_id,
                    duration_secs,
                    width,
                    height,
                    fps,
                    video_codec.as_deref(),
                    audio_codec.as_deref(),
                    audio_channels,
                    bitrate_kbps,
                    format.as_deref(),
                )
                .await
                .ok();

                log_event(
                    &self.pool,
                    video_id,
                    "ffprobe_done",
                    Some(&format!(
                        "{}x{} {:?}fps duration={:?}s",
                        width.unwrap_or(0),
                        height.unwrap_or(0),
                        fps,
                        duration_secs
                    )),
                )
                .await
                .ok();
            }
            Err(e) => {
                log_event(&self.pool, video_id, "ffprobe_warn", Some(&e))
                    .await
                    .ok();
                // Non-fatal: continue to transcoding even without probe data
            }
        }

        // ── 2. Transcode each rendition ──────────────────────────────────────
        let mut master_entries: Vec<String> = Vec::new();
        let mut any_ok = false;

        for rendition in &self.renditions {
            let rend_id = insert_rendition(
                &self.pool,
                video_id,
                &rendition.label,
                rendition.width as i64,
                rendition.height as i64,
                rendition.video_bitrate_kbps as i64,
            )
            .await
            .unwrap_or(0);

            let rend_dir = output_dir.join(&rendition.label);
            if let Err(e) = tokio::fs::create_dir_all(&rend_dir).await {
                let msg = format!("mkdir failed for {}: {e}", rendition.label);
                log_event(&self.pool, video_id, "rendition_failed", Some(&msg))
                    .await
                    .ok();
                if rend_id > 0 {
                    mark_rendition_failed(&self.pool, rend_id, &msg).await.ok();
                }
                continue;
            }

            let segment_pattern = rend_dir.join("segment%03d.ts").to_string_lossy().into_owned();
            let playlist_path = rend_dir.join("index.m3u8").to_string_lossy().into_owned();

            log_event(
                &self.pool,
                video_id,
                "ffmpeg_start",
                Some(&format!("rendition={}", rendition.label)),
            )
            .await
            .ok();

            let result = run_ffmpeg_hls(
                &self.ffmpeg_bin,
                input,
                &playlist_path,
                &segment_pattern,
                rendition,
            )
            .await;

            match result {
                Ok(stderr_tail) => {
                    // Count segments produced
                    let seg_count = count_ts_segments(&rend_dir).await;
                    let playlist_url =
                        format!("/stream/{video_id}/{}/index.m3u8", rendition.label);

                    if rend_id > 0 {
                        mark_rendition_done(&self.pool, rend_id, &playlist_url, seg_count)
                            .await
                            .ok();
                    }

                    log_event(
                        &self.pool,
                        video_id,
                        "rendition_done",
                        Some(&format!(
                            "label={} segments={seg_count}",
                            rendition.label
                        )),
                    )
                    .await
                    .ok();

                    // Append to master playlist
                    master_entries.push(format!(
                        "#EXT-X-STREAM-INF:BANDWIDTH={},RESOLUTION={}x{}\n{}/index.m3u8",
                        rendition.video_bitrate_kbps * 1000,
                        rendition.width,
                        rendition.height,
                        rendition.label,
                    ));
                    any_ok = true;

                    let _ = stderr_tail; // could be logged at TRACE if desired
                }
                Err(e) => {
                    log_event(&self.pool, video_id, "ffmpeg_failed", Some(&e))
                        .await
                        .ok();
                    if rend_id > 0 {
                        mark_rendition_failed(&self.pool, rend_id, &e).await.ok();
                    }
                    // Continue trying remaining renditions
                }
            }
        }

        if !any_ok {
            return Err("All renditions failed – check ffmpeg_failed events for details.".into());
        }

        // ── 3. Write master.m3u8 ────────────────────────────────────────────
        let master_content = format!("#EXTM3U\n#EXT-X-VERSION:3\n\n{}\n", master_entries.join("\n\n"));
        let master_path = output_dir.join("master.m3u8");
        tokio::fs::write(&master_path, master_content)
            .await
            .map_err(|e| format!("Failed to write master.m3u8: {e}"))?;

        log_event(
            &self.pool,
            video_id,
            "ffmpeg_done",
            Some(&format!("master.m3u8 written with {} renditions", master_entries.len())),
        )
        .await
        .ok();

        Ok(())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn run_ffprobe(bin: &Path, input: &str) -> Result<ProbeOutput, String> {
    let output = Command::new(bin)
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            input,
        ])
        .output()
        .await
        .map_err(|e| format!("ffprobe launch failed: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("ffprobe exited with error: {stderr}"));
    }

    serde_json::from_slice::<ProbeOutput>(&output.stdout)
        .map_err(|e| format!("ffprobe JSON parse error: {e}"))
}

async fn run_ffmpeg_hls(
    bin: &Path,
    input: &str,
    playlist_path: &str,
    segment_pattern: &str,
    rendition: &RenditionConfig,
) -> Result<String, String> {
    let vf = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2",
        rendition.width, rendition.height, rendition.width, rendition.height
    );

    let output = Command::new(bin)
        .args([
            "-i", input,
            "-vf", &vf,
            "-c:v", "libx264",
            "-preset", "fast",
            "-crf", "23",
            "-b:v", &format!("{}k", rendition.video_bitrate_kbps),
            "-maxrate", &format!("{}k", rendition.video_bitrate_kbps),
            "-bufsize", &format!("{}k", rendition.video_bitrate_kbps * 2),
            "-c:a", "aac",
            "-b:a", &format!("{}k", rendition.audio_bitrate_kbps),
            "-ar", "48000",
            "-hls_time", "6",
            "-hls_playlist_type", "vod",
            "-hls_segment_filename", segment_pattern,
            "-hls_flags", "independent_segments",
            "-y",
            playlist_path,
        ])
        .output()
        .await
        .map_err(|e| format!("ffmpeg launch failed: {e}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        // Return last 2KB of stderr as the error context
        let tail: String = stderr.chars().rev().take(2048).collect::<String>().chars().rev().collect();
        return Err(format!("ffmpeg exited {}: {}", output.status, tail));
    }

    Ok(stderr)
}

async fn count_ts_segments(dir: &Path) -> i64 {
    let mut count = 0i64;
    if let Ok(mut rd) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = rd.next_entry().await {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("ts") {
                count += 1;
            }
        }
    }
    count
}

/// Parse "num/den" rational frame-rate string into f64.
fn parse_fps(s: &str) -> Option<f64> {
    let mut parts = s.splitn(2, '/');
    let num: f64 = parts.next()?.parse().ok()?;
    let den: f64 = parts.next()?.parse().ok()?;
    if den == 0.0 { None } else { Some(num / den) }
}
