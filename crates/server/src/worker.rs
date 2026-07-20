use crate::{
    core::{
        jobs::JobStatusStore,
        traits::{Queue, Transcoder},
    },
    db::{queries, SqlitePool},
};
use std::{path::PathBuf, sync::Arc};

pub async fn start_worker(
    queue: Arc<dyn Queue>,
    transcoder: Arc<dyn Transcoder>,
    jobs: JobStatusStore,
    output_base_dir: PathBuf,
    pool: Arc<SqlitePool>,
) {
    println!("Background Job Worker started...");

    loop {
        match queue.consume().await {
            Ok(job) => {
                println!("Worker picked up job: {}", job.id);

                // Update in-memory cache + SQLite
                jobs.processing(&job.id).await;
                queries::update_video_status(&pool, &job.id, "processing", None)
                    .await
                    .ok();
                queries::log_event(&pool, &job.id, "processing", None)
                    .await
                    .ok();

                let output_dir = output_base_dir.join(&job.id);

                if let Err(e) = tokio::fs::create_dir_all(&output_dir).await {
                    let msg = e.to_string();
                    eprintln!("Failed to create output dir for job {}: {}", job.id, msg);
                    jobs.failed(&job.id, msg.clone()).await;
                    queries::update_video_status(&pool, &job.id, "failed", Some(&msg))
                        .await
                        .ok();
                    queries::log_event(&pool, &job.id, "failed", Some(&msg))
                        .await
                        .ok();
                    continue;
                }

                match transcoder.transcode(&job, &output_dir).await {
                    Ok(_) => {
                        let manifest_url =
                            format!("/stream/{}/master.m3u8", job.id);
                        jobs.ready(&job.id).await;
                        queries::update_video_manifest(
                            &pool,
                            &job.id,
                            &manifest_url,
                            Some(&job.id),
                        )
                        .await
                        .ok();
                        queries::log_event(&pool, &job.id, "ready", Some(&manifest_url))
                            .await
                            .ok();
                        println!("Successfully transcoded job {}", job.id);
                    }
                    Err(e) => {
                        eprintln!("Failed to transcode job {}: {}", job.id, e);
                        jobs.failed(&job.id, e.clone()).await;
                        queries::update_video_status(&pool, &job.id, "failed", Some(&e))
                            .await
                            .ok();
                        queries::log_event(&pool, &job.id, "failed", Some(&e))
                            .await
                            .ok();
                    }
                }
            }
            Err(e) => {
                eprintln!("Queue closed or error: {}. Shutting down worker...", e);
                break;
            }
        }
    }
}
