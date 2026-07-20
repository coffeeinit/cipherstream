use crate::{
    api::{self, AppState},
    config::Config,
    core::{
        jobs::JobStatusStore,
        queue::MemoryQueue,
        storage::create_storage,
        traits::Storage,
    },
    db,
    transcoder::NativeFFmpegTranscoder,
    worker,
};
use std::{path::PathBuf, sync::Arc};
use tokio::spawn;

pub async fn run_server() -> Result<(), String> {
    let config = Config::load()?;
    let upload_dir = config.upload_dir();
    let hls_dir = config.hls_dir();

    // Ensure data directories exist
    std::fs::create_dir_all(&upload_dir)
        .map_err(|e| format!("Failed to create upload directory: {e}"))?;
    std::fs::create_dir_all(&hls_dir)
        .map_err(|e| format!("Failed to create HLS directory: {e}"))?;

    // Initialise SQLite connection pool + apply schema
    let pool = Arc::new(
        db::init_pool(&config.database.path)
            .await
            .map_err(|e| format!("Database init failed: {e}"))?,
    );
    println!("SQLite database ready at {}", config.database.path.display());

    // Spawn the embedded TUS server if enabled
    if config.tus.enabled {
        spawn_tus_server(&config, upload_dir.clone());
    }

    let queue = Arc::new(MemoryQueue::new(100));
    let storage: Arc<dyn Storage> = Arc::from(create_storage(&config.storage)?);
    let jobs = JobStatusStore::default();

    // Build the FFmpeg CLI transcoder with config-driven rendition ladder
    let ffmpeg_transcoder = Arc::new(NativeFFmpegTranscoder::new(
        config.ffmpeg_bin(),
        config.ffprobe_bin(),
        config.transcoder.renditions.clone(),
        pool.clone(),
    ));

    // Start the background transcoding worker
    let worker_queue = queue.clone();
    let worker_transcoder = ffmpeg_transcoder.clone();
    let worker_jobs = jobs.clone();
    let worker_pool = pool.clone();
    let output_base_dir = hls_dir.clone();
    spawn(async move {
        worker::start_worker(
            worker_queue,
            worker_transcoder,
            worker_jobs,
            output_base_dir,
            worker_pool,
        )
        .await;
    });

    let state = Arc::new(AppState {
        queue,
        storage,
        jobs,
        upload_dir,
        hls_dir,
        db: pool,
    });

    let app = api::create_router(state);
    let listener = tokio::net::TcpListener::bind(config.server.bind_addr)
        .await
        .map_err(|e| format!("Failed to bind server: {e}"))?;

    println!(
        "CipherStream listening on http://{}",
        listener
            .local_addr()
            .map_err(|e| format!("Failed to read listener address: {e}"))?
    );

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("CipherStream server stopped with error: {e}"))
}

fn spawn_tus_server(config: &Config, upload_dir: PathBuf) {
    let binary_path = config
        .tus
        .binary_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("./target/release/rustus"));
    let host = config.tus.host.clone();
    let port = config.tus.port.to_string();

    println!(
        "Booting embedded TUS server ({}) on {}:{}...",
        binary_path.display(),
        host,
        port
    );

    tokio::spawn(async move {
        let mut child = tokio::process::Command::new(binary_path)
            .arg("--host")
            .arg(host)
            .arg("--port")
            .arg(port)
            .arg("--data-dir")
            .arg(upload_dir)
            .spawn()
            .expect("Failed to start rustus TUS server");

        child.wait().await.unwrap();
    });
}
