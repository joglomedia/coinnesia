pub mod rate_limiter;

use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use chrono::Utc;
use rust_decimal::Decimal;
use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    cache::{
        snapshots::{ScanSnapshot, SignalSnapshot},
        Cache,
    },
    config::AppConfig,
    data::{
        proxy,
        stream::{CandleEvent, CandleStream},
        CandleRequest, MarketDataSource, PerSymbolMarketData,
    },
    storage::{alerts::AlertJobRecord, signals::SignalRecord, Db},
    strategy::{mtf::parse_timeframe, SignalDirection, SignalResult},
    AssetClass, Timeframe,
};

#[derive(Debug, Clone)]
pub struct ScanReport {
    pub scanned: usize,
    pub signals: Vec<SignalResult>,
    pub cycle_id: Uuid,
}

pub struct Scanner {
    config: AppConfig,
    data_source: Arc<dyn MarketDataSource>,
    publisher: ScanPublisher,
}

impl Scanner {
    pub fn new(config: AppConfig) -> Self {
        let data_source = Arc::new(PerSymbolMarketData::from_config(&config));
        Self {
            config,
            data_source,
            publisher: ScanPublisher::default(),
        }
    }

    pub fn with_data_source(config: AppConfig, data_source: Arc<dyn MarketDataSource>) -> Self {
        Self {
            config,
            data_source,
            publisher: ScanPublisher::default(),
        }
    }

    pub fn with_resources(
        config: AppConfig,
        data_source: Arc<dyn MarketDataSource>,
        db: Option<Db>,
        cache: Option<Cache>,
    ) -> Self {
        Self {
            config,
            data_source,
            publisher: ScanPublisher { db, cache },
        }
    }

    pub async fn scan_once(&self) -> Result<ScanReport> {
        let cycle_id = Uuid::new_v4();
        let started_at = Utc::now();
        let work_items = self.ingest().await?;
        let signals = self.analyze(work_items).await?;
        let report = ScanReport {
            scanned: self.config.symbols.len(),
            signals,
            cycle_id,
        };
        self.publisher
            .publish(&self.config, &report, started_at, Utc::now())
            .await?;
        Ok(report)
    }

    async fn ingest(&self) -> Result<Vec<ScanWorkItem>> {
        let candle_limit = self.config.data_sources.candle_limit;
        let _proxy_snapshot = proxy::fetch_once_per_cycle(
            self.data_source.as_ref(),
            &self.config.proxy_symbols,
            Timeframe::D1,
            candle_limit,
        )
        .await
        .unwrap_or_default();

        let requests = self
            .config
            .symbols
            .iter()
            .map(|symbol_config| {
                let timeframe = symbol_config
                    .timeframes
                    .first()
                    .and_then(|timeframe| parse_timeframe(timeframe))
                    .unwrap_or(Timeframe::D1);
                CandleRequest::new(symbol_config.symbol.clone(), timeframe, candle_limit)
            })
            .collect::<Vec<_>>();
        let candles = self.data_source.batch_candles(&requests).await?;

        Ok(self
            .config
            .symbols
            .iter()
            .map(|symbol_config| {
                let timeframe = symbol_config
                    .timeframes
                    .first()
                    .and_then(|timeframe| parse_timeframe(timeframe))
                    .unwrap_or(Timeframe::D1);
                let request =
                    CandleRequest::new(symbol_config.symbol.clone(), timeframe, candle_limit);
                ScanWorkItem {
                    symbol: symbol_config.symbol.clone(),
                    timeframe,
                    asset_class: asset_class_name(symbol_config.asset_class).to_owned(),
                    candles: candles.get(&request).cloned().unwrap_or_default(),
                }
            })
            .collect())
    }

    async fn analyze(&self, work_items: Vec<ScanWorkItem>) -> Result<Vec<SignalResult>> {
        let semaphore = Arc::new(Semaphore::new(self.config.runtime.max_symbol_tasks.max(1)));
        let mut tasks = Vec::with_capacity(work_items.len());

        for item in work_items {
            let permit = semaphore.clone().acquire_owned().await?;
            let strategy_config = self.config.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = permit;
                debug!(
                    symbol = %item.symbol,
                    timeframe = ?item.timeframe,
                    asset_class = %item.asset_class,
                    "analyzing scan work item"
                );
                if item.candles.is_empty() {
                    return SignalResult::wait(&item.symbol, "market_data_unavailable");
                }
                let engine = crate::strategy::StrategyEngine::new(strategy_config);
                engine.evaluate(&item.symbol, &item.candles)
            }));
        }

        let mut signals = Vec::with_capacity(tasks.len());
        for task in tasks {
            signals.push(task.await.context("scanner analysis task failed")?);
        }
        Ok(signals)
    }

    pub async fn run(&self) -> Result<()> {
        let mut interval =
            tokio::time::interval(Duration::from_secs(self.config.runtime.scan_interval_secs));
        loop {
            interval.tick().await;
            let report = self.scan_once().await?;
            info!(
                scanned = report.scanned,
                signals = report.signals.len(),
                "scanner loop completed one cycle"
            );
        }
    }

    pub async fn run_until_cancelled(&self, shutdown: CancellationToken) -> Result<()> {
        let mut interval =
            tokio::time::interval(Duration::from_secs(self.config.runtime.scan_interval_secs));
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return Ok(()),
                _ = interval.tick() => {
                    let report = self.scan_once().await?;
                    info!(
                        scanned = report.scanned,
                        signals = report.signals.len(),
                        "scanner loop completed one cycle"
                    );
                }
            }
        }
    }

    /// Event-driven scanner: reacts to closed candle events from a `CandleStream`.
    ///
    /// Before entering the event loop, seeds the stream buffer with historical
    /// candles via REST so that indicator calculations have enough history from
    /// the very first event.
    pub async fn run_streaming(
        &self,
        stream: Arc<dyn CandleStream>,
        shutdown: CancellationToken,
    ) -> Result<()> {
        // Seed stream buffers with historical candles before subscribing
        info!("streaming scanner: priming candle buffers with historical data");
        let candle_limit = self.config.data_sources.candle_limit;
        for symbol_config in &self.config.symbols {
            let timeframe = symbol_config
                .timeframes
                .first()
                .and_then(|tf| parse_timeframe(tf))
                .unwrap_or(Timeframe::H1);
            match self.data_source.candles(&symbol_config.symbol, timeframe, candle_limit).await {
                Ok(candles) if !candles.is_empty() => {
                    stream.prime(&symbol_config.symbol, candles, timeframe).await;
                }
                Ok(_) => warn!(symbol = %symbol_config.symbol, "no historical candles to prime"),
                Err(e) => warn!(symbol = %symbol_config.symbol, error = %e, "prime fetch failed"),
            }
        }
        info!("streaming scanner: buffers primed, entering event loop");

        let mut rx = stream.subscribe();
        let stream_task = {
            let s = stream.clone();
            let sd = shutdown.clone();
            tokio::spawn(async move { s.run(sd).await })
        };

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                event = rx.recv() => {
                    match event {
                        Err(_) => break,
                        Ok(CandleEvent { is_closed: false, .. }) => continue,
                        Ok(event) => {
                            self.handle_streaming_event(&event, stream.as_ref()).await;
                        }
                    }
                }
            }
        }

        stream_task.await.context("stream task panicked")??;
        Ok(())
    }

    async fn handle_streaming_event(&self, event: &CandleEvent, stream: &dyn CandleStream) {
        let candles = stream.candles(&event.symbol, event.timeframe);
        if candles.is_empty() {
            return;
        }

        let asset_class = self
            .config
            .symbols
            .iter()
            .find(|s| s.symbol == event.symbol)
            .map(|s| asset_class_name(s.asset_class).to_owned())
            .unwrap_or_else(|| "unknown".to_owned());

        let work_item = ScanWorkItem {
            symbol: event.symbol.clone(),
            timeframe: event.timeframe,
            asset_class,
            candles,
        };

        match self.analyze(vec![work_item]).await {
            Ok(signals) => {
                let cycle_id = Uuid::new_v4();
                let now = Utc::now();
                let report = ScanReport { scanned: 1, signals, cycle_id };
                if let Err(e) = self.publisher.publish(&self.config, &report, now, now).await {
                    tracing::error!(symbol = %event.symbol, error = %e, "failed to publish streaming signal");
                }
            }
            Err(e) => {
                tracing::error!(symbol = %event.symbol, error = %e, "streaming analysis failed");
            }
        }
    }
}

#[derive(Clone)]
struct ScanWorkItem {
    symbol: String,
    timeframe: Timeframe,
    asset_class: String,
    candles: Vec<crate::Candle>,
}

#[derive(Clone, Default)]
pub struct ScanPublisher {
    db: Option<Db>,
    cache: Option<Cache>,
}

impl ScanPublisher {
    async fn publish(
        &self,
        config: &AppConfig,
        report: &ScanReport,
        started_at: chrono::DateTime<Utc>,
        completed_at: chrono::DateTime<Utc>,
    ) -> Result<()> {
        let Some(db) = self.db.clone() else {
            self.cache_snapshots(report, started_at, completed_at)
                .await?;
            return Ok(());
        };
        let cache = self.cache.clone();
        let (tx, mut rx) = mpsc::channel::<PublishJob>(config.runtime.max_symbol_tasks.max(1));
        let writer = tokio::spawn(async move {
            while let Some(job) = rx.recv().await {
                match job {
                    PublishJob::Signal(signal) => db.signals().append(&signal).await?,
                    PublishJob::Alert(alert) => db.alerts().append_job(&alert).await?,
                }
            }
            Result::<()>::Ok(())
        });

        self.cache_snapshots(report, started_at, completed_at)
            .await?;
        for signal in &report.signals {
            let record = signal_record(config, report.cycle_id, signal, completed_at);
            let alert = alert_job(&record, signal, completed_at);
            tx.send(PublishJob::Signal(record))
                .await
                .context("failed to enqueue signal persistence job")?;
            tx.send(PublishJob::Alert(alert))
                .await
                .context("failed to enqueue alert persistence job")?;
            if let Some(cache) = &cache {
                let _ = cache
                    .publish_json(
                        "signals",
                        &SignalSnapshot::from_signal(report.cycle_id, signal),
                    )
                    .await;
            }
        }
        drop(tx);
        writer.await.context("publisher worker join failed")??;
        Ok(())
    }

    async fn cache_snapshots(
        &self,
        report: &ScanReport,
        started_at: chrono::DateTime<Utc>,
        completed_at: chrono::DateTime<Utc>,
    ) -> Result<()> {
        let Some(cache) = &self.cache else {
            return Ok(());
        };
        let snapshot = ScanSnapshot {
            cycle_id: report.cycle_id.to_string(),
            scanned: report.scanned,
            signals: report.signals.len(),
            started_at,
            completed_at,
        };
        cache
            .set_json(
                &cache.key(&["snapshot", "scan", "latest"]),
                &snapshot,
                cache.default_ttl(),
            )
            .await?;
        for signal in &report.signals {
            cache
                .set_json(
                    &cache.key(&["snapshot", "signal", &signal.symbol]),
                    &SignalSnapshot::from_signal(report.cycle_id, signal),
                    cache.default_ttl(),
                )
                .await?;
        }
        Ok(())
    }
}

#[derive(Debug)]
enum PublishJob {
    Signal(SignalRecord),
    Alert(AlertJobRecord),
}

impl SignalSnapshot {
    fn from_signal(cycle_id: Uuid, signal: &SignalResult) -> Self {
        Self {
            cycle_id: cycle_id.to_string(),
            signal: signal.clone(),
            updated_at: Utc::now(),
        }
    }
}

fn signal_record(
    config: &AppConfig,
    cycle_id: Uuid,
    signal: &SignalResult,
    evaluated_at: chrono::DateTime<Utc>,
) -> SignalRecord {
    let symbol_config = config
        .symbols
        .iter()
        .find(|item| item.symbol == signal.symbol);
    let timeframe = symbol_config
        .and_then(|item| item.timeframes.first())
        .cloned();
    let asset_class = symbol_config.map(|item| asset_class_name(item.asset_class).to_owned());
    let confidence = signal.confidence.long.max(signal.confidence.short);
    let direction = match SignalDirection::from(signal.state) {
        SignalDirection::Long => "long",
        SignalDirection::Short => "short",
        SignalDirection::Wait => "wait",
    };
    let mut record = SignalRecord::new(
        Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("{cycle_id}:{}", signal.symbol).as_bytes(),
        ),
        signal.symbol.clone(),
        format!("{:?}", signal.state).to_uppercase(),
        direction,
        decimal(confidence),
        decimal(signal.confidence.directional_gap),
        signal.reason.clone(),
        evaluated_at,
    );
    record.timeframe = timeframe;
    record.asset_class = asset_class;
    record.entry_plan = signal
        .entry_plan
        .as_ref()
        .and_then(|entry_plan| serde_json::to_value(entry_plan).ok());
    record.indicators = serde_json::json!({
        "confidence": signal.confidence,
        "cycle_id": cycle_id,
    });
    record
}

fn alert_job(
    signal_record: &SignalRecord,
    signal: &SignalResult,
    scheduled_at: chrono::DateTime<Utc>,
) -> AlertJobRecord {
    AlertJobRecord::pending(
        Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("alert:{}", signal_record.id).as_bytes(),
        ),
        Some(signal_record.id),
        "telegram",
        serde_json::json!({
            "signal": signal,
            "message": crate::alerts::telegram::format_signal(signal),
            "dedupe_key": format!(
                "telegram:{}:{}:{}",
                signal.symbol,
                signal_record.timeframe.as_deref().unwrap_or("unknown"),
                signal_record.state
            ),
        }),
        Some(format!("telegram:{}", signal_record.id)),
        scheduled_at,
    )
}

fn decimal(value: f64) -> Decimal {
    Decimal::from_f64_retain(value).unwrap_or(Decimal::ZERO)
}

fn asset_class_name(asset_class: AssetClass) -> &'static str {
    match asset_class {
        AssetClass::Btc => "btc",
        AssetClass::Altcoin => "altcoin",
        AssetClass::Gold => "gold",
        AssetClass::Forex => "forex",
        AssetClass::StocksIdx => "stocks_idx",
        AssetClass::StocksUs => "stocks_us",
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use async_trait::async_trait;
    use chrono::{Duration, Utc};

    use crate::{Candle, Timeframe};

    use super::*;

    #[derive(Debug, Clone)]
    struct MockDataSource {
        candles: Vec<Candle>,
    }

    #[async_trait]
    impl MarketDataSource for MockDataSource {
        async fn candles(
            &self,
            _symbol: &str,
            _timeframe: Timeframe,
            _limit: usize,
        ) -> Result<Vec<Candle>> {
            Ok(self.candles.clone())
        }
    }

    #[tokio::test]
    async fn scanner_fetches_candles_before_evaluating_strategy() {
        let config = AppConfig::from_default_toml().expect("default config parses");
        let now = Utc::now();
        let candles = (0..20)
            .map(|idx| Candle {
                ts: now + Duration::minutes(idx),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.5,
                volume: 1000.0,
            })
            .collect::<Vec<_>>();
        let scanner = Scanner::with_data_source(config, Arc::new(MockDataSource { candles }));
        let report = scanner.scan_once().await.expect("scan succeeds");

        assert_eq!(report.scanned, 4);
        assert!(report
            .signals
            .iter()
            .all(|signal| signal.reason != "market_data_unavailable"));
    }
}
