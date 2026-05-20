use anyhow::Result;
use async_trait::async_trait;

use crate::{alerts::AlertSink, strategy::SignalResult};

#[derive(Debug, Clone, Default)]
pub struct TelegramAlertSink;

#[async_trait]
impl AlertSink for TelegramAlertSink {
    async fn send(&self, _signal: &SignalResult) -> Result<()> {
        Ok(())
    }
}

pub fn format_signal(signal: &SignalResult) -> String {
    let tp3_note = signal
        .entry_plan
        .as_ref()
        .map(|_| "TP3 optional")
        .unwrap_or("no entry plan");
    format!(
        "{} {:?} confidence L:{:.1} S:{:.1} ({})",
        signal.symbol, signal.state, signal.confidence.long, signal.confidence.short, tp3_note
    )
}
