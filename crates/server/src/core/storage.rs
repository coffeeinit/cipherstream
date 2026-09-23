use super::traits::Storage;
use crate::config::StorageConfig;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Local filesystem storage is the only enabled backend in the beta.
/// S3 can be added behind this same trait after the local path is stable.
pub fn create_storage(config: &StorageConfig) -> Result<Box<dyn Storage>, String> {
    if config.r#type != "local" {
        return Err(format!(
            "Storage backend `{}` is not enabled in beta; use storage.type = \"local\".",
            config.r#type
        ));
    }
    LocalStorage::new(config.data_dir.clone())
        .map(|storage| Box::new(storage) as Box<dyn Storage>)
        .map_err(|e| format!("Failed to initialize local storage: {e}"))
}

pub struct LocalStorage {
    base_dir: PathBuf,
}

impl LocalStorage {
    pub fn new(base_dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }
}

#[async_trait]
impl Storage for LocalStorage {
    async fn put(&self, key: &str, file_path: &Path) -> Result<(), String> {
        let dest = self.base_dir.join(key);
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
        }
        tokio::fs::copy(file_path, &dest).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn get(&self, key: &str, dest_path: &Path) -> Result<(), String> {
        tokio::fs::copy(self.base_dir.join(key), dest_path)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn signed_url(&self, key: &str, _ttl: Duration) -> Result<String, String> {
        Ok(format!("/stream/{key}"))
    }
}
