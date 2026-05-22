pub mod telegram;
pub mod worker;

use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait AlertSink: Send + Sync {
    async fn send(&self, message: &str) -> Result<()>;
}
