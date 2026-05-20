pub mod telegram;

use anyhow::Result;
use async_trait::async_trait;

use crate::strategy::SignalResult;

#[async_trait]
pub trait AlertSink: Send + Sync {
    async fn send(&self, signal: &SignalResult) -> Result<()>;
}
