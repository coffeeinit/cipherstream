use std::path::Path;
use std::time::Duration;
use async_trait::async_trait;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Job {
    pub id: String,
    pub input_file: String,
}

#[async_trait]
pub trait Storage: Send + Sync {
    async fn put(&self, key: &str, file_path: &Path) -> Result<(), String>;
    async fn get(&self, key: &str, dest_path: &Path) -> Result<(), String>;
    async fn signed_url(&self, key: &str, ttl: Duration) -> Result<String, String>;
}

#[async_trait]
pub trait Queue: Send + Sync {
    async fn publish(&self, job: Job) -> Result<(), String>;
    async fn consume(&self) -> Result<Job, String>;
}

#[async_trait]
pub trait Transcoder: Send + Sync {
    async fn transcode(&self, job: &Job, output_dir: &Path) -> Result<(), String>;
}

#[async_trait]
pub trait Auth: Send + Sync {
    async fn authorize(&self, user_id: &str, video_id: &str) -> bool;
}
