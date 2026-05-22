use std::{collections::VecDeque, time::Duration};

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use futures::StreamExt;
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    config::BinanceWsConfig,
    data::{
        binance_interval,
        stream::{buffer_key, new_buffers, CandleBuffers, CandleEvent, CandleStream},
    },
    Candle, Timeframe,
};

const BROADCAST_CAPACITY: usize = 1024;

/// Binance combined-stream WebSocket client.
///
/// Connects to `wss://stream.binance.com/stream?streams=...` and maintains a
/// per-symbol candle ring-buffer. Events are broadcast on each kline update.
/// Auto-reconnects with exponential backoff on disconnect.
pub struct BinanceWsStream {
    config: BinanceWsConfig,
    symbols: Vec<(String, Timeframe)>,
    buffers: CandleBuffers,
    tx: broadcast::Sender<CandleEvent>,
}

impl BinanceWsStream {
    pub fn new(config: BinanceWsConfig, symbols: Vec<(String, Timeframe)>) -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            config,
            symbols,
            buffers: new_buffers(),
            tx,
        }
    }

    /// Build the combined-stream URL for a chunk of (symbol, timeframe) pairs.
    fn stream_url(&self, chunk: &[(String, Timeframe)]) -> String {
        let streams = chunk
            .iter()
            .map(|(symbol, tf)| format!("{}@kline_{}", symbol.to_lowercase(), binance_interval(*tf)))
            .collect::<Vec<_>>()
            .join("/");
        format!("{}?streams={}", self.config.url, streams)
    }

    async fn connect_and_read(
        url: &str,
        tx: &broadcast::Sender<CandleEvent>,
        buffers: &CandleBuffers,
        buffer_size: usize,
        shutdown: &CancellationToken,
    ) -> Result<()> {
        info!(url, "Binance WebSocket connecting");
        let (ws_stream, _) = connect_async(url)
            .await
            .with_context(|| format!("WebSocket connect failed: {url}"))?;
        info!(url, "Binance WebSocket connected");

        let (_, mut read) = ws_stream.split();

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    info!("Binance WebSocket shutting down gracefully");
                    return Ok(());
                }
                msg = read.next() => {
                    match msg {
                        None => {
                            warn!("Binance WebSocket stream closed by server");
                            return Err(anyhow::anyhow!("WebSocket stream ended"));
                        }
                        Some(Err(e)) => {
                            warn!(error = %e, "Binance WebSocket read error");
                            return Err(e.into());
                        }
                        Some(Ok(Message::Text(text))) => {
                            if let Err(e) = handle_message(&text, tx, buffers, buffer_size).await {
                                debug!(error = %e, "failed to handle WebSocket message");
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            debug!("received Binance WebSocket ping");
                            // tokio-tungstenite auto-responds to pings
                            let _ = data;
                        }
                        Some(Ok(_)) => {}
                    }
                }
            }
        }
    }
}

async fn handle_message(
    text: &str,
    tx: &broadcast::Sender<CandleEvent>,
    buffers: &CandleBuffers,
    buffer_size: usize,
) -> Result<()> {
    let envelope: StreamEnvelope = serde_json::from_str(text).context("parse stream envelope")?;
    let kline = envelope.data.k;

    let timeframe = parse_interval(&envelope.data.event_type, &kline.interval);
    let candle = Candle {
        ts: Utc
            .timestamp_millis_opt(kline.open_time)
            .single()
            .context("invalid kline timestamp")?,
        open: kline.open.parse().context("parse open")?,
        high: kline.high.parse().context("parse high")?,
        low: kline.low.parse().context("parse low")?,
        close: kline.close.parse().context("parse close")?,
        volume: kline.volume.parse().context("parse volume")?,
    };
    let symbol = kline.symbol.clone();
    let is_closed = kline.is_closed;

    // Update ring buffer
    {
        let key = buffer_key(&symbol, timeframe);
        let mut map = buffers.write().await;
        let buf = map.entry(key).or_insert_with(VecDeque::new);
        if is_closed {
            buf.push_back(candle.clone());
            while buf.len() > buffer_size {
                buf.pop_front();
            }
        } else if let Some(last) = buf.back_mut() {
            // Update in-progress candle (same open_time) or append
            if last.ts == candle.ts {
                *last = candle.clone();
            }
        }
    }

    let _ = tx.send(CandleEvent {
        symbol,
        timeframe,
        candle,
        is_closed,
    });

    Ok(())
}

#[async_trait]
impl CandleStream for BinanceWsStream {
    fn subscribe(&self) -> broadcast::Receiver<CandleEvent> {
        self.tx.subscribe()
    }

    async fn prime(&self, symbol: &str, candles: Vec<Candle>, timeframe: Timeframe) {
        let key = buffer_key(symbol, timeframe);
        let mut map = self.buffers.write().await;
        let buf = map.entry(key).or_insert_with(VecDeque::new);
        buf.clear();
        buf.extend(candles);
        while buf.len() > self.config.candle_buffer_size {
            buf.pop_front();
        }
    }

    fn candles(&self, symbol: &str, timeframe: Timeframe) -> Vec<Candle> {
        let key = buffer_key(symbol, timeframe);
        if let Ok(map) = self.buffers.try_read() {
            map.get(&key)
                .map(|buf| buf.iter().cloned().collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    fn symbols(&self) -> Vec<(String, Timeframe)> {
        self.symbols.clone()
    }

    async fn run(&self, shutdown: CancellationToken) -> Result<()> {
        let chunk_size = self.config.max_streams_per_connection.max(1);
        let chunks: Vec<Vec<(String, Timeframe)>> = self
            .symbols
            .chunks(chunk_size)
            .map(|c| c.to_vec())
            .collect();

        if chunks.is_empty() {
            info!("BinanceWsStream: no symbols configured, stream idle");
            shutdown.cancelled().await;
            return Ok(());
        }

        let mut join_set: tokio::task::JoinSet<Result<()>> = tokio::task::JoinSet::new();
        for chunk in chunks {
            let url = self.stream_url(&chunk);
            let tx = self.tx.clone();
            let buffers = self.buffers.clone();
            let buffer_size = self.config.candle_buffer_size;
            let base_delay = self.config.reconnect_base_delay_ms;
            let max_delay = self.config.reconnect_max_delay_ms;
            let shutdown = shutdown.clone();

            join_set.spawn(async move {
                let mut delay = Duration::from_millis(base_delay);
                let max_delay = Duration::from_millis(max_delay);
                loop {
                    match Self::connect_and_read(&url, &tx, &buffers, buffer_size, &shutdown).await {
                        Ok(()) => return Ok(()),
                        Err(e) => {
                            if shutdown.is_cancelled() {
                                return Ok(());
                            }
                            error!(error = %e, reconnect_in_ms = delay.as_millis(), "WebSocket disconnected");
                            tokio::select! {
                                _ = shutdown.cancelled() => return Ok(()),
                                _ = tokio::time::sleep(delay) => {
                                    delay = (delay * 2).min(max_delay);
                                }
                            }
                        }
                    }
                }
            });
        }

        while let Some(result) = join_set.join_next().await {
            if let Err(e) = result {
                error!(error = ?e, "WebSocket connection task panicked");
            }
        }
        Ok(())
    }
}

/// Minimum interval mapping for Binance kline interval strings.
fn parse_interval(_event_type: &str, interval: &str) -> Timeframe {
    match interval {
        "1m" => Timeframe::M1,
        "5m" => Timeframe::M5,
        "15m" => Timeframe::M15,
        "1h" => Timeframe::H1,
        "4h" => Timeframe::H4,
        "1d" => Timeframe::D1,
        "1w" => Timeframe::W1,
        "1M" => Timeframe::Mn1,
        _ => Timeframe::M15,
    }
}

// ─── Binance combined-stream message structures ────────────────────────────

#[derive(Debug, Deserialize)]
struct StreamEnvelope {
    data: KlineData,
}

#[derive(Debug, Deserialize)]
struct KlineData {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "k")]
    k: KlinePayload,
}

#[derive(Debug, Deserialize)]
struct KlinePayload {
    #[serde(rename = "t")]
    open_time: i64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "i")]
    interval: String,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "x")]
    is_closed: bool,
}
