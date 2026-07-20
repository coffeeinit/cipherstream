use super::traits::Storage;
use crate::config::StorageConfig;
use std::path::{Path, PathBuf};
use std::time::Duration;
use async_trait::async_trait;

// ── Factory ───────────────────────────────────────────────────────────────────

pub fn create_storage(config: &StorageConfig) -> Result<Box<dyn Storage>, String> {
    match config.r#type.as_str() {
        "local" => LocalStorage::new(config.data_dir.clone())
            .map(|s| Box::new(s) as Box<dyn Storage>)
            .map_err(|e| format!("Failed to initialize local storage: {e}")),
        "s3" => {
            let bucket = config
                .bucket
                .clone()
                .ok_or("storage.bucket is required for s3 backend")?;
            let region = config
                .region
                .clone()
                .unwrap_or_else(|| "us-east-1".to_string());
            let endpoint = config.endpoint.clone();
            let access_key = config.access_key_id.clone()
                .or_else(|| std::env::var("AWS_ACCESS_KEY_ID").ok());
            let secret_key = config.secret_access_key.clone()
                .or_else(|| std::env::var("AWS_SECRET_ACCESS_KEY").ok());

            Ok(Box::new(S3Storage {
                bucket,
                region,
                endpoint,
                access_key,
                secret_key,
            }))
        }
        other => Err(format!("Unsupported storage type `{other}`. Use `local` or `s3`.")),
    }
}

// ── LocalStorage ─────────────────────────────────────────────────────────────

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
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }
        tokio::fs::copy(file_path, &dest)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn get(&self, key: &str, dest_path: &Path) -> Result<(), String> {
        let src = self.base_dir.join(key);
        tokio::fs::copy(&src, dest_path)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn signed_url(&self, key: &str, _ttl: Duration) -> Result<String, String> {
        // For local storage, the "signed URL" is simply a direct route path.
        Ok(format!("/stream/{}", key))
    }
}

// ── S3Storage ─────────────────────────────────────────────────────────────────

pub struct S3Storage {
    bucket: String,
    region: String,
    endpoint: Option<String>,
    access_key: Option<String>,
    secret_key: Option<String>,
}

impl S3Storage {
    async fn client(&self) -> Result<aws_sdk_s3::Client, String> {
        use aws_config::Region;
        use aws_credential_types::Credentials;

        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(Region::new(self.region.clone()));

        if let (Some(ak), Some(sk)) = (self.access_key.clone(), self.secret_key.clone()) {
            let creds = Credentials::new(ak, sk, None, None, "cipherstream-config");
            loader = loader.credentials_provider(creds);
        }

        let mut sdk_config = loader.load().await;

        // Build client, optionally overriding the endpoint for MinIO / local S3
        let mut builder = aws_sdk_s3::config::Builder::from(&sdk_config)
            .force_path_style(true); // required for MinIO

        if let Some(ep) = &self.endpoint {
            builder = builder.endpoint_url(ep.clone());
        }

        Ok(aws_sdk_s3::Client::from_conf(builder.build()))
    }
}

#[async_trait]
impl Storage for S3Storage {
    async fn put(&self, key: &str, file_path: &Path) -> Result<(), String> {
        let client = self.client().await?;
        let body = aws_sdk_s3::primitives::ByteStream::from_path(file_path)
            .await
            .map_err(|e| format!("S3 put – read file: {e}"))?;

        client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(body)
            .send()
            .await
            .map_err(|e| format!("S3 put_object: {e}"))?;

        Ok(())
    }

    async fn get(&self, key: &str, dest_path: &Path) -> Result<(), String> {
        let client = self.client().await?;
        let resp = client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| format!("S3 get_object: {e}"))?;

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| format!("S3 body collect: {e}"))?
            .into_bytes();

        tokio::fs::write(dest_path, bytes)
            .await
            .map_err(|e| format!("S3 get – write file: {e}"))?;

        Ok(())
    }

    async fn signed_url(&self, key: &str, ttl: Duration) -> Result<String, String> {
        use aws_sdk_s3::presigning::PresigningConfig;

        let client = self.client().await?;
        let presigning = PresigningConfig::expires_in(ttl)
            .map_err(|e| format!("S3 presigning config: {e}"))?;

        let url = client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presigning)
            .await
            .map_err(|e| format!("S3 presign: {e}"))?
            .uri()
            .to_string();

        Ok(url)
    }
}
