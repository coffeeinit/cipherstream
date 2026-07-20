use super::traits::{Job, Queue};
use tokio::sync::mpsc;
use async_trait::async_trait;

pub struct MemoryQueue {
    sender: mpsc::Sender<Job>,
    // We use a Mutex over the receiver because consume() requires mutable access to the receiver,
    // but the Queue trait takes &self.
    receiver: tokio::sync::Mutex<mpsc::Receiver<Job>>,
}

impl MemoryQueue {
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::channel(capacity);
        Self {
            sender,
            receiver: tokio::sync::Mutex::new(receiver),
        }
    }
}

#[async_trait]
impl Queue for MemoryQueue {
    async fn publish(&self, job: Job) -> Result<(), String> {
        self.sender
            .send(job)
            .await
            .map_err(|e| format!("Failed to publish job: {}", e))
    }

    async fn consume(&self) -> Result<Job, String> {
        let mut rx = self.receiver.lock().await;
        rx.recv()
            .await
            .ok_or_else(|| "Queue closed".to_string())
    }
}
