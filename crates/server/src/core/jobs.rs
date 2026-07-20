use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Processing,
    Ready,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
pub struct VideoStatus {
    pub video_id: String,
    pub state: JobState,
    pub playlist_url: String,
    pub source_url: String,
    pub error: Option<String>,
}

#[derive(Clone, Default)]
pub struct JobStatusStore {
    inner: Arc<RwLock<HashMap<String, VideoStatus>>>,
}

impl JobStatusStore {
    pub async fn queued(&self, video_id: &str) {
        self.set(VideoStatus {
            video_id: video_id.to_string(),
            state: JobState::Queued,
            playlist_url: format!("/stream/{video_id}/playlist.m3u8"),
            source_url: format!("/stream/{video_id}/source.mp4"),
            error: None,
        })
        .await;
    }

    pub async fn processing(&self, video_id: &str) {
        self.update(video_id, JobState::Processing, None).await;
    }

    pub async fn ready(&self, video_id: &str) {
        self.update(video_id, JobState::Ready, None).await;
    }

    pub async fn failed(&self, video_id: &str, error: String) {
        self.update(video_id, JobState::Failed, Some(error)).await;
    }

    pub async fn get(&self, video_id: &str) -> Option<VideoStatus> {
        self.inner.read().await.get(video_id).cloned()
    }

    async fn set(&self, status: VideoStatus) {
        self.inner
            .write()
            .await
            .insert(status.video_id.clone(), status);
    }

    async fn update(&self, video_id: &str, state: JobState, error: Option<String>) {
        if let Some(status) = self.inner.write().await.get_mut(video_id) {
            status.state = state;
            status.error = error;
        }
    }
}
