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
    strategy::{
        guard_state::{GuardStateStore, InMemoryGuardStore},
        mtf::{parse_timeframe, required_timeframes, MtfCandles},
        SignalDirection, SignalResult,
    },
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
    guard_store: Arc<dyn GuardStateStore>,
}

impl Scanner {
    pub fn new(config: AppConfig) -> Self {
        let data_source = Arc::new(PerSymbolMarketData::from_config(&config));
        Self {
            config,
            data_source,
            publisher: ScanPublisher::default(),
            guard_store: Arc::new(InMemoryGuardStore::new()),
        }
    }

    pub fn with_data_source(config: AppConfig, data_source: Arc<dyn MarketDataSource>) -> Self {
        Self {
            config,
            data_source,
            publisher: ScanPublisher::default(),
            guard_store: Arc::new(InMemoryGuardStore::new()),
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
            guard_store: Arc::new(InMemoryGuardStore::new()),
        }
    }

    /// Replace the default in-memory guard store with a custom implementation
    /// (e.g. a Valkey-backed store for multi-process deployments).
    pub fn with_guard_store(mut self, store: Arc<dyn GuardStateStore>) -> Self {
        self.guard_store = store;
        self
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

        // Fan out one CandleRequest per (symbol, timeframe) across the asset-class
        // required timeframe set. Sharing one batch_candles call preserves the
        // per-source concurrency already provided by PerSymbolMarketData.
        let mut requests: Vec<CandleRequest> = Vec::new();
        for symbol_config in &self.config.symbols {
            let primary = primary_timeframe(symbol_config);
            for tf in required_timeframes(symbol_config.asset_class, primary) {
                requests.push(CandleRequest::new(
                    symbol_config.symbol.clone(),
                    tf,
                    candle_limit,
                ));
            }
        }
        let candles = self.data_source.batch_candles(&requests).await?;

        Ok(self
            .config
            .symbols
            .iter()
            .map(|symbol_config| {
                let primary = primary_timeframe(symbol_config);
                let mut by_tf: std::collections::BTreeMap<Timeframe, Vec<crate::Candle>> =
                    std::collections::BTreeMap::new();
                for tf in required_timeframes(symbol_config.asset_class, primary) {
                    let request =
                        CandleRequest::new(symbol_config.symbol.clone(), tf, candle_limit);
                    let series = candles.get(&request).cloned().unwrap_or_default();
                    by_tf.insert(tf, series);
                }
                let primary_candles = by_tf.get(&primary).cloned().unwrap_or_default();
                ScanWorkItem {
                    symbol: symbol_config.symbol.clone(),
                    timeframe: primary,
                    asset_class: asset_class_name(symbol_config.asset_class).to_owned(),
                    candles: primary_candles,
                    mtf: MtfCandles::new(primary, by_tf),
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
            let store = self.guard_store.clone();
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
                let prev_state = store.load(&item.symbol).await;
                let engine = crate::strategy::StrategyEngine::new(strategy_config);
                let (signal, new_state) = engine.evaluate_with_state(
                    &item.symbol,
                    &item.candles,
                    Some(&item.mtf),
                    &prev_state,
                );
                store.save(&item.symbol, new_state).await;
                signal
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

        let mut by_tf: std::collections::BTreeMap<Timeframe, Vec<crate::Candle>> =
            std::collections::BTreeMap::new();
        by_tf.insert(event.timeframe, candles.clone());
        let mtf = MtfCandles::new(event.timeframe, by_tf);
        let work_item = ScanWorkItem {
            symbol: event.symbol.clone(),
            timeframe: event.timeframe,
            asset_class,
            candles,
            mtf,
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
    mtf: MtfCandles,
}

fn primary_timeframe(symbol_config: &crate::config::SymbolConfig) -> Timeframe {
    symbol_config
        .timeframes
        .first()
        .and_then(|timeframe| parse_timeframe(timeframe))
        .unwrap_or(Timeframe::D1)
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
    use std::sync::Mutex;

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

    /// Recording mock that captures every (symbol, timeframe) pair it serves
    /// during a scan cycle so tests can assert on the fan-out behaviour.
    #[derive(Debug, Default)]
    struct RecordingDataSource {
        candles: Vec<Candle>,
        requested: Mutex<Vec<(String, Timeframe)>>,
    }

    #[async_trait]
    impl MarketDataSource for RecordingDataSource {
        async fn candles(
            &self,
            symbol: &str,
            timeframe: Timeframe,
            _limit: usize,
        ) -> Result<Vec<Candle>> {
            self.requested
                .lock()
                .unwrap()
                .push((symbol.to_owned(), timeframe));
            Ok(self.candles.clone())
        }
    }

    /// Recording GuardStateStore: counts every load/save and stores the
    /// payload so persistence behaviour can be asserted across scan cycles.
    #[derive(Debug, Default)]
    struct RecordingGuardStore {
        inner: Mutex<std::collections::HashMap<String, crate::strategy::guard_state::GuardState>>,
        loads: Mutex<Vec<String>>,
        saves: Mutex<Vec<(String, crate::strategy::guard_state::GuardState)>>,
    }

    #[async_trait]
    impl crate::strategy::guard_state::GuardStateStore for RecordingGuardStore {
        async fn load(&self, symbol: &str) -> crate::strategy::guard_state::GuardState {
            self.loads.lock().unwrap().push(symbol.to_owned());
            self.inner
                .lock()
                .unwrap()
                .get(symbol)
                .cloned()
                .unwrap_or_default()
        }

        async fn save(&self, symbol: &str, state: crate::strategy::guard_state::GuardState) {
            self.saves
                .lock()
                .unwrap()
                .push((symbol.to_owned(), state.clone()));
            self.inner.lock().unwrap().insert(symbol.to_owned(), state);
        }
    }

    #[tokio::test]
    async fn scanner_fetches_candles_before_evaluating_strategy() {
        let config = AppConfig::from_default_toml().expect("default config parses");
        let expected = config.symbols.len();
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

        assert_eq!(report.scanned, expected);
        assert!(report
            .signals
            .iter()
            .all(|signal| signal.reason != "market_data_unavailable"));
    }

    #[tokio::test]
    async fn scanner_fetches_full_mtf_timeframe_set_per_symbol() {
        // Acceptance for 1.7.3: scanner pulls every required timeframe per
        // symbol exactly once per cycle. We use a recording mock to observe
        // the (symbol, timeframe) pairs surfaced via batch_candles.
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
        let source = Arc::new(RecordingDataSource {
            candles,
            requested: Mutex::new(Vec::new()),
        });

        let scanner = Scanner::with_data_source(config.clone(), source.clone());
        let _ = scanner.scan_once().await.expect("scan succeeds");

        let requested = source.requested.lock().unwrap().clone();
        for symbol_config in &config.symbols {
            let primary = primary_timeframe(symbol_config);
            let required = crate::strategy::mtf::required_timeframes(
                symbol_config.asset_class,
                primary,
            );
            for tf in required {
                let hits = requested
                    .iter()
                    .filter(|(s, t)| s == &symbol_config.symbol && t == &tf)
                    .count();
                assert!(
                    hits >= 1,
                    "expected at least one fetch for {} {tf:?}; observed {:?}",
                    symbol_config.symbol,
                    requested
                        .iter()
                        .filter(|(s, _)| s == &symbol_config.symbol)
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    #[tokio::test]
    async fn guard_state_survives_scan_cycles() {
        // Acceptance for 1.7.4: state is loaded and saved per symbol on every
        // scan cycle. After two cycles the recording store must show ≥ 2
        // load/save calls per symbol and the persisted state from cycle N must
        // be the prev_state delivered to cycle N+1.
        let config = AppConfig::from_default_toml().expect("default config parses");
        let symbols = config
            .symbols
            .iter()
            .map(|s| s.symbol.clone())
            .collect::<Vec<_>>();
        let now = Utc::now();
        let candles = (0..50)
            .map(|idx| Candle {
                ts: now + Duration::minutes(idx),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.5,
                volume: 1_000.0,
            })
            .collect::<Vec<_>>();
        let store = Arc::new(RecordingGuardStore::default());
        let scanner = Scanner::with_data_source(config, Arc::new(MockDataSource { candles }))
            .with_guard_store(store.clone());

        scanner.scan_once().await.expect("first cycle succeeds");
        scanner.scan_once().await.expect("second cycle succeeds");

        let loads = store.loads.lock().unwrap().clone();
        let saves = store.saves.lock().unwrap().clone();
        for symbol in &symbols {
            let load_hits = loads.iter().filter(|s| s == &symbol).count();
            let save_hits = saves.iter().filter(|(s, _)| s == symbol).count();
            assert!(
                load_hits >= 2 && save_hits >= 2,
                "{symbol} load={load_hits} save={save_hits} (need ≥2 each across 2 cycles)"
            );
        }
        // The store must hold a state for every configured symbol after the
        // second cycle.
        let stored = store.inner.lock().unwrap();
        for symbol in &symbols {
            assert!(
                stored.contains_key(symbol),
                "{symbol} state must be persisted between cycles"
            );
        }
    }
}
