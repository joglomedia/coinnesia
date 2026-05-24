pub mod binance;
pub mod binance_ws;
pub mod proxy;
pub mod retry;
pub mod stream;
pub mod tradingview;
pub mod twelvedata;

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{config::AppConfig, Candle, Timeframe};

// ─── Core trait ───────────────────────────────────────────────────────────────

#[async_trait]
pub trait MarketDataSource: Send + Sync {
    async fn candles(&self, symbol: &str, timeframe: Timeframe, limit: usize) -> Result<Vec<Candle>>;

    async fn batch_candles(
        &self,
        requests: &[CandleRequest],
    ) -> Result<BTreeMap<CandleRequest, Vec<Candle>>> {
        let mut output = BTreeMap::new();
        for request in requests {
            let candles = self
                .candles(&request.symbol, request.timeframe, request.limit)
                .await?;
            output.insert(request.clone(), candles);
        }
        Ok(output)
    }

    async fn quote(&self, symbol: &str) -> Result<Option<Quote>> {
        let candles = self.candles(symbol, Timeframe::D1, 1).await?;
        Ok(candles.last().map(|candle| Quote {
            symbol: symbol.to_owned(),
            price: candle.close,
            ts: candle.ts,
        }))
    }
}

// ─── Supporting types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CandleRequest {
    pub symbol: String,
    pub timeframe: Timeframe,
    pub limit: usize,
}

impl CandleRequest {
    pub fn new(symbol: impl Into<String>, timeframe: Timeframe, limit: usize) -> Self {
        Self { symbol: symbol.into(), timeframe, limit }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    pub symbol: String,
    pub price: f64,
    pub ts: DateTime<Utc>,
}

// ─── Empty source ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct EmptyDataSource;

#[async_trait]
impl MarketDataSource for EmptyDataSource {
    async fn candles(&self, _: &str, _: Timeframe, _: usize) -> Result<Vec<Candle>> {
        Ok(Vec::new())
    }
}

// ─── PerSymbolMarketData ──────────────────────────────────────────────────────

/// Routes each symbol to its configured data source, based on `SymbolConfig.data_source`
/// and `ProxySymbolEntry.source` in TOML config.
///
/// `batch_candles` runs Binance, TradingView, and Twelve Data groups **concurrently**.
/// TradingView's own within-group batching (by timeframe) is preserved.
pub struct PerSymbolMarketData {
    binance:     Arc<dyn MarketDataSource>,
    tradingview: Arc<dyn MarketDataSource>,
    twelvedata:  Arc<dyn MarketDataSource>,
    routing:     HashMap<String, RouteSource>,
    global_fallback: RouteSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RouteSource {
    Binance,
    TradingView,
    TwelveData,
}

impl PerSymbolMarketData {
    pub fn from_config(config: &AppConfig) -> Self {
        let retry = config.data_sources.retry.clone();
        let rate_limit = config.exchange.rate_limit_per_second as u32;

        let binance: Arc<dyn MarketDataSource> = Arc::new(binance::BinanceDataSource::new(
            config.exchange.binance.clone(),
            rate_limit,
            retry.clone(),
        ));
        let tradingview: Arc<dyn MarketDataSource> = Arc::new(
            tradingview::TradingViewDataSource::new(
                config.data_sources.tradingview.clone(),
                retry.clone(),
            ),
        );
        let twelvedata: Arc<dyn MarketDataSource> = Arc::new(
            twelvedata::TwelveDataDataSource::new(config.data_sources.twelvedata.clone(), retry),
        );

        let global_fallback = route_source_from_str(&config.data_sources.fallback);
        let mut routing: HashMap<String, RouteSource> = HashMap::new();

        // Route main trading symbols
        for sym in &config.symbols {
            let src = sym
                .data_source
                .as_deref()
                .unwrap_or_else(|| default_source_for_exchange(&sym.exchange, &config.data_sources.primary));
            routing.insert(sym.symbol.to_uppercase(), route_source_from_str(src));
        }

        // Route proxy symbols
        let px = &config.proxy_symbols;
        for entry in [&px.xauusd, &px.ihsg, &px.dxy] {
            let source = route_source_from_str(&entry.source);
            routing.insert(entry.symbol().to_uppercase(), source);
        }

        Self { binance, tradingview, twelvedata, routing, global_fallback }
    }

    fn adapter(&self, source: RouteSource) -> &Arc<dyn MarketDataSource> {
        match source {
            RouteSource::Binance     => &self.binance,
            RouteSource::TradingView => &self.tradingview,
            RouteSource::TwelveData  => &self.twelvedata,
        }
    }

    fn resolve(&self, symbol: &str) -> RouteSource {
        self.routing
            .get(&symbol.to_uppercase())
            .copied()
            .unwrap_or(self.global_fallback)
    }
}

#[async_trait]
impl MarketDataSource for PerSymbolMarketData {
    async fn candles(&self, symbol: &str, timeframe: Timeframe, limit: usize) -> Result<Vec<Candle>> {
        self.adapter(self.resolve(symbol)).candles(symbol, timeframe, limit).await
    }

    async fn batch_candles(
        &self,
        requests: &[CandleRequest],
    ) -> Result<BTreeMap<CandleRequest, Vec<Candle>>> {
        let mut binance_reqs: Vec<CandleRequest> = Vec::new();
        let mut tv_reqs:      Vec<CandleRequest> = Vec::new();
        let mut td_reqs:      Vec<CandleRequest> = Vec::new();

        for req in requests {
            match self.resolve(&req.symbol) {
                RouteSource::Binance     => binance_reqs.push(req.clone()),
                RouteSource::TradingView => tv_reqs.push(req.clone()),
                RouteSource::TwelveData  => td_reqs.push(req.clone()),
            }
        }

        // Fetch all source groups concurrently
        let (binance_res, tv_res, td_res) = tokio::join!(
            batch_group(&self.binance,     &binance_reqs),
            batch_group(&self.tradingview, &tv_reqs),
            batch_group(&self.twelvedata,  &td_reqs),
        );

        let mut output: BTreeMap<CandleRequest, Vec<Candle>> = BTreeMap::new();
        output.extend(binance_res);
        output.extend(tv_res);
        output.extend(td_res);
        Ok(output)
    }
}

// ─── ConfiguredMarketData ─────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ConfiguredMarketData {
    primary:  DataProvider,
    fallback: DataProvider,
}

#[derive(Clone)]
enum DataProvider {
    Binance(binance::BinanceDataSource),
    TradingView(tradingview::TradingViewDataSource),
    TwelveData(twelvedata::TwelveDataDataSource),
    Empty(EmptyDataSource),
}

impl ConfiguredMarketData {
    pub fn from_config(config: &AppConfig) -> Self {
        let retry = config.data_sources.retry.clone();
        let rate_limit = config.exchange.rate_limit_per_second as u32;
        let primary  = provider_from_name(&config.data_sources.primary,  config, rate_limit, retry.clone());
        let fallback = provider_from_name(&config.data_sources.fallback, config, rate_limit, retry);
        Self { primary, fallback }
    }
}

#[async_trait]
impl MarketDataSource for ConfiguredMarketData {
    async fn candles(&self, symbol: &str, timeframe: Timeframe, limit: usize) -> Result<Vec<Candle>> {
        match self.primary.candles(symbol, timeframe, limit).await {
            Ok(candles) if !candles.is_empty() => Ok(candles),
            Ok(_) | Err(_) => self.fallback.candles(symbol, timeframe, limit).await,
        }
    }
}

#[async_trait]
impl MarketDataSource for DataProvider {
    async fn candles(&self, symbol: &str, timeframe: Timeframe, limit: usize) -> Result<Vec<Candle>> {
        match self {
            Self::Binance(s)     => s.candles(symbol, timeframe, limit).await,
            Self::TradingView(s) => s.candles(symbol, timeframe, limit).await,
            Self::TwelveData(s)  => s.candles(symbol, timeframe, limit).await,
            Self::Empty(s)       => s.candles(symbol, timeframe, limit).await,
        }
    }
}

fn provider_from_name(
    name: &str,
    config: &AppConfig,
    rate_limit: u32,
    retry: crate::config::RetryConfig,
) -> DataProvider {
    match name {
        "binance" => DataProvider::Binance(binance::BinanceDataSource::new(
            config.exchange.binance.clone(),
            rate_limit,
            retry,
        )),
        "tradingview" => DataProvider::TradingView(tradingview::TradingViewDataSource::new(
            config.data_sources.tradingview.clone(),
            retry,
        )),
        "twelvedata" => DataProvider::TwelveData(twelvedata::TwelveDataDataSource::new(
            config.data_sources.twelvedata.clone(),
            retry,
        )),
        _ => DataProvider::Empty(EmptyDataSource),
    }
}

// ─── WebSocket stream factory ─────────────────────────────────────────────────

/// Build a `BinanceWsStream` from config if WebSocket streaming is enabled.
pub fn binance_ws_stream(config: &AppConfig) -> Option<binance_ws::BinanceWsStream> {
    if !config.exchange.binance.ws.enabled {
        return None;
    }
    let symbols = config
        .symbols
        .iter()
        .filter(|s| {
            let src = s
                .data_source
                .as_deref()
                .unwrap_or_else(|| default_source_for_exchange(&s.exchange, &config.data_sources.primary));
            src == "binance"
        })
        .map(|s| {
            let timeframe = s
                .timeframes
                .first()
                .and_then(|tf| parse_timeframe(tf).ok())
                .unwrap_or(Timeframe::H1);
            (s.symbol.clone(), timeframe)
        })
        .collect::<Vec<_>>();

    if symbols.is_empty() {
        return None;
    }
    Some(binance_ws::BinanceWsStream::new(config.exchange.binance.ws.clone(), symbols))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn default_source_for_exchange<'a>(exchange: &str, global_primary: &'a str) -> &'a str {
    match exchange.to_lowercase().as_str() {
        "binance"     => "binance",
        "tradingview" => "tradingview",
        "twelvedata"  => "twelvedata",
        _             => global_primary,
    }
}

fn route_source_from_str(name: &str) -> RouteSource {
    match name {
        "binance"     => RouteSource::Binance,
        "tradingview" => RouteSource::TradingView,
        "twelvedata"  => RouteSource::TwelveData,
        _             => RouteSource::TwelveData, // default fallback
    }
}

async fn batch_group(
    source: &Arc<dyn MarketDataSource>,
    reqs: &[CandleRequest],
) -> BTreeMap<CandleRequest, Vec<Candle>> {
    if reqs.is_empty() {
        return BTreeMap::new();
    }
    source.batch_candles(reqs).await.unwrap_or_default()
}

pub fn parse_timeframe(value: &str) -> Result<Timeframe> {
    use anyhow::anyhow;
    match value {
        "1m" | "M1"          => Ok(Timeframe::M1),
        "5m" | "M5"          => Ok(Timeframe::M5),
        "15m" | "M15"        => Ok(Timeframe::M15),
        "1h" | "H1"          => Ok(Timeframe::H1),
        "4h" | "H4"          => Ok(Timeframe::H4),
        "1d" | "D1"          => Ok(Timeframe::D1),
        "1w" | "W1"          => Ok(Timeframe::W1),
        "1mo" | "MN1" | "Mn1" => Ok(Timeframe::Mn1),
        _ => Err(anyhow!("unsupported timeframe {value}")),
    }
}

pub fn binance_interval(timeframe: Timeframe) -> &'static str {
    match timeframe {
        Timeframe::M1  => "1m",
        Timeframe::M5  => "5m",
        Timeframe::M15 => "15m",
        Timeframe::H1  => "1h",
        Timeframe::H4  => "4h",
        Timeframe::D1  => "1d",
        Timeframe::W1  => "1w",
        Timeframe::Mn1 => "1M",
    }
}
