use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use coinnesia::{
    cache::{snapshots::ScanSnapshot, Cache},
    config::{CacheConfig, DatabaseConfig},
    data::MarketDataSource,
    scanner::Scanner,
    storage::Db,
    Candle, Timeframe,
};
use uuid::Uuid;

#[derive(Debug, Clone)]
struct CountingDataSource {
    candles: Vec<Candle>,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl MarketDataSource for CountingDataSource {
    async fn candles(
        &self,
        _symbol: &str,
        _timeframe: Timeframe,
        _limit: usize,
    ) -> Result<Vec<Candle>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.candles.clone())
    }
}

fn fixture_candles() -> Vec<Candle> {
    let now = Utc::now();
    (0..260)
        .map(|idx| {
            let close = 100.0 + idx as f64 * 0.4;
            Candle {
                ts: now + Duration::minutes(idx),
                open: close - 0.2,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 1_000.0 + idx as f64,
            }
        })
        .collect()
}

fn test_database_config() -> Option<DatabaseConfig> {
    std::env::var("DATABASE_URL").ok()?;
    Some(DatabaseConfig {
        enabled: true,
        url_env: "DATABASE_URL".to_owned(),
        max_connections: 2,
        min_connections: 0,
        connect_timeout_secs: 5,
        migrate_on_start: true,
    })
}

fn test_cache_config() -> Option<CacheConfig> {
    std::env::var("VALKEY_URL").ok()?;
    Some(CacheConfig {
        enabled: true,
        url_env: "VALKEY_URL".to_owned(),
        key_prefix: format!("coinnesia_test:{}", Uuid::new_v4()),
        pool_size: 1,
        ttl_seconds: 60,
    })
}

#[tokio::test]
async fn scanner_prefetches_proxy_data_once_then_scans_configured_symbols() {
    let mut config = coinnesia::config::AppConfig::from_default_toml().expect("config parses");
    config.runtime.max_symbol_tasks = 1;
    config.proxy_symbols.xauusd = coinnesia::config::ProxySymbolEntry::from_binance("PAXGUSDT");
    config.proxy_symbols.ihsg = coinnesia::config::ProxySymbolEntry::from_binance("BTCUSDT");
    config.proxy_symbols.dxy = coinnesia::config::ProxySymbolEntry::from_binance("ETHUSDT");

    let calls = Arc::new(AtomicUsize::new(0));
    let scanner = Scanner::with_data_source(
        config.clone(),
        Arc::new(CountingDataSource {
            candles: fixture_candles(),
            calls: calls.clone(),
        }),
    );

    let report = scanner.scan_once().await.expect("scan succeeds");

    assert_eq!(report.scanned, config.symbols.len());
    assert_eq!(report.signals.len(), config.symbols.len());
    assert_eq!(calls.load(Ordering::SeqCst), config.symbols.len() + 3);
}

#[tokio::test]
async fn scanner_caches_snapshots_when_valkey_url_is_set() {
    let Some(cache_config) = test_cache_config() else {
        return;
    };

    let mut config = coinnesia::config::AppConfig::from_default_toml().expect("config parses");
    config.cache = cache_config;
    config.proxy_symbols.xauusd = coinnesia::config::ProxySymbolEntry::from_binance("PAXGUSDT");
    config.proxy_symbols.ihsg = coinnesia::config::ProxySymbolEntry::from_binance("BTCUSDT");
    config.proxy_symbols.dxy = coinnesia::config::ProxySymbolEntry::from_binance("ETHUSDT");

    let cache = Cache::connect_optional(&config.cache)
        .await
        .expect("Valkey connects")
        .expect("cache enabled");
    let scanner = Scanner::with_resources(
        config,
        Arc::new(CountingDataSource {
            candles: fixture_candles(),
            calls: Arc::new(AtomicUsize::new(0)),
        }),
        None,
        Some(cache.clone()),
    );

    let report = scanner.scan_once().await.expect("scan succeeds");

    let snapshot: ScanSnapshot = cache
        .get_json(&cache.key(&["snapshot", "scan", "latest"]))
        .await
        .expect("scan snapshot reads")
        .expect("scan snapshot exists");
    assert_eq!(snapshot.cycle_id, report.cycle_id.to_string());

    let signal_snapshot: serde_json::Value = cache
        .get_json(&cache.key(&["snapshot", "signal", "BTCUSDT"]))
        .await
        .expect("signal snapshot reads")
        .expect("signal snapshot exists");
    let cycle_id = report.cycle_id.to_string();
    assert_eq!(
        signal_snapshot["cycle_id"].as_str(),
        Some(cycle_id.as_str())
    );
}

#[tokio::test]
async fn scanner_persists_signal_evaluations_and_alert_jobs_when_database_url_is_set() {
    let Some(database_config) = test_database_config() else {
        return;
    };

    let mut config = coinnesia::config::AppConfig::from_default_toml().expect("config parses");
    config.database = database_config;
    let symbol = format!("TEST{}USDT", Uuid::new_v4().simple());
    config.symbols[0].symbol = symbol.clone();
    config.symbols.truncate(1);
    config.proxy_symbols.xauusd = coinnesia::config::ProxySymbolEntry::from_binance("PAXGUSDT");
    config.proxy_symbols.ihsg = coinnesia::config::ProxySymbolEntry::from_binance("BTCUSDT");
    config.proxy_symbols.dxy = coinnesia::config::ProxySymbolEntry::from_binance("ETHUSDT");

    let db = Db::connect_optional(&config.database)
        .await
        .expect("database connects")
        .expect("database enabled");
    let scanner = Scanner::with_resources(
        config,
        Arc::new(CountingDataSource {
            candles: fixture_candles(),
            calls: Arc::new(AtomicUsize::new(0)),
        }),
        Some(db.clone()),
        None,
    );

    let report = scanner.scan_once().await.expect("scan succeeds");

    let signal = db
        .signals()
        .get_by_cycle_symbol(report.cycle_id, &symbol)
        .await
        .expect("persisted signal reads");
    assert_eq!(signal.symbol, symbol);

    let alert_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("alert:{}", signal.id).as_bytes(),
    );
    let alert = db
        .alerts()
        .get_job(alert_id)
        .await
        .expect("alert job reads");
    assert_eq!(alert.signal_id, Some(signal.id));
    assert_eq!(alert.status, "pending");
    assert_eq!(alert.channel, "telegram");
    assert_eq!(alert.dedupe_key, Some(format!("telegram:{}", signal.id)));
    assert!(alert.payload["dedupe_key"]
        .as_str()
        .unwrap_or_default()
        .starts_with("telegram:"));
}
