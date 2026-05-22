use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::{Candle, Timeframe};

/// A candle event emitted by a streaming data source.
#[derive(Debug, Clone)]
pub struct CandleEvent {
    pub symbol: String,
    pub timeframe: Timeframe,
    pub candle: Candle,
    /// True when the bar has closed; false for intra-bar (live) updates.
    pub is_closed: bool,
}

/// Abstraction over any streaming candle data source.
///
/// Implementors maintain a per-symbol candle buffer and broadcast events on each
/// update. The scanner subscribes to closed-bar events to drive strategy evaluation.
#[async_trait]
pub trait CandleStream: Send + Sync {
    /// Subscribe to future candle events from this stream.
    fn subscribe(&self) -> broadcast::Receiver<CandleEvent>;

    /// Seed the internal buffer with historical candles before streaming begins.
    /// Called once per symbol after connecting but before the scanner loop starts.
    async fn prime(&self, symbol: &str, candles: Vec<Candle>, timeframe: Timeframe);

    /// Return the buffered candles for a symbol (used by scanner during strategy eval).
    fn candles(&self, symbol: &str, timeframe: Timeframe) -> Vec<Candle>;

    /// Symbols that this stream covers.
    fn symbols(&self) -> Vec<(String, Timeframe)>;

    /// Connect and stream indefinitely until `shutdown` is cancelled.
    async fn run(&self, shutdown: CancellationToken) -> Result<()>;
}

/// Helper to build a buffer key from symbol + timeframe.
pub(crate) fn buffer_key(symbol: &str, timeframe: Timeframe) -> String {
    format!("{}:{:?}", symbol.to_uppercase(), timeframe)
}

/// Shared candle ring-buffer map: `"BTCUSDT:M15"` → `VecDeque<Candle>`.
pub(crate) type CandleBuffers =
    std::sync::Arc<tokio::sync::RwLock<HashMap<String, std::collections::VecDeque<Candle>>>>;

pub(crate) fn new_buffers() -> CandleBuffers {
    std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new()))
}
