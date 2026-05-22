pub mod binance;
pub mod binance_ws;
pub mod proxy;
pub mod retry;
pub mod stream;
pub mod tradingview;
pub mod yahoo;

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future;
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
/// Outperforms `ConfiguredMarketData` because:
/// - No wasted fallback attempts for well-known source/symbol pairs
/// - `batch_candles` runs Binance, TradingView, and Yahoo groups **concurrently**
/// - TradingView's own within-group batching (by timeframe) is preserved
pub struct PerSymbolMarketData {
    binance: Arc<dyn MarketDataSource>,
    tradingview: Arc<dyn MarketDataSource>,
    yahoo: Arc<dyn MarketDataSource>,
    routing: HashMap<String, SymbolRoute>,
    global_fallback: RouteSource,
}

#[derive(Clone, Debug)]
struct SymbolRoute {
    source: RouteSource,
    /// When primary returns empty, retry on Yahoo with this symbol
    yahoo_fallback: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RouteSource {
    Binance,
    TradingView,
    Yahoo,
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
        let yahoo: Arc<dyn MarketDataSource> =
            Arc::new(yahoo::YahooDataSource::new(config.data_sources.yahoo.clone(), retry));

        let global_fallback = route_source_from_str(&config.data_sources.fallback);
        let mut routing: HashMap<String, SymbolRoute> = HashMap::new();

        // Route main trading symbols
        for sym in &config.symbols {
            let src = sym
                .data_source
                .as_deref()
                .unwrap_or_else(|| default_source_for_exchange(&sym.exchange, &config.data_sources.primary));
            routing.insert(
                sym.symbol.to_uppercase(),
                SymbolRoute { source: route_source_from_str(src), yahoo_fallback: None },
            );
        }

        // Route proxy symbols with fallback
        let px = &config.proxy_symbols;
        for entry in [&px.xauusd, &px.ihsg, &px.dxy] {
            let source = route_source_from_str(&entry.source);
            let primary = entry.symbol().to_uppercase();
            let yahoo_fb = if source != RouteSource::Yahoo {
                Some(entry.yahoo.clone())
            } else {
                None
            };
            routing.insert(primary.clone(), SymbolRoute { source, yahoo_fallback: yahoo_fb });
            // Register Yahoo symbol too so direct Yahoo calls always resolve
            let yahoo_key = entry.yahoo.to_uppercase();
            if yahoo_key != primary {
                routing.entry(yahoo_key).or_insert(SymbolRoute {
                    source: RouteSource::Yahoo,
                    yahoo_fallback: None,
                });
            }
        }

        Self { binance, tradingview, yahoo, routing, global_fallback }
    }

    fn adapter(&self, source: RouteSource) -> &Arc<dyn MarketDataSource> {
        match source {
            RouteSource::Binance => &self.binance,
            RouteSource::TradingView => &self.tradingview,
            RouteSource::Yahoo => &self.yahoo,
        }
    }

    fn resolve(&self, symbol: &str) -> (RouteSource, Option<&str>) {
        match self.routing.get(&symbol.to_uppercase()) {
            Some(r) => (r.source, r.yahoo_fallback.as_deref()),
            None => (self.global_fallback, None),
        }
    }
}

#[async_trait]
impl MarketDataSource for PerSymbolMarketData {
    async fn candles(&self, symbol: &str, timeframe: Timeframe, limit: usize) -> Result<Vec<Candle>> {
        let (source, yahoo_fb) = self.resolve(symbol);
        match self.adapter(source).candles(symbol, timeframe, limit).await {
            Ok(c) if !c.is_empty() => return Ok(c),
            _ => {}
        }
        if let Some(fb_sym) = yahoo_fb {
            return self.yahoo.candles(fb_sym, timeframe, limit).await;
        }
        Ok(Vec::new())
    }

    async fn batch_candles(
        &self,
        requests: &[CandleRequest],
    ) -> Result<BTreeMap<CandleRequest, Vec<Candle>>> {
        let mut binance_reqs: Vec<CandleRequest> = Vec::new();
        let mut tv_reqs: Vec<CandleRequest> = Vec::new();
        let mut yahoo_reqs: Vec<CandleRequest> = Vec::new();
        let mut fallback_map: HashMap<CandleRequest, String> = HashMap::new();

        for req in requests {
            let (source, yahoo_fb) = self.resolve(&req.symbol);
            match source {
                RouteSource::Binance => binance_reqs.push(req.clone()),
                RouteSource::TradingView => {
                    tv_reqs.push(req.clone());
                    if let Some(fb) = yahoo_fb {
                        fallback_map.insert(req.clone(), fb.to_owned());
                    }
                }
                RouteSource::Yahoo => yahoo_reqs.push(req.clone()),
            }
        }

        // Fetch all source groups concurrently
        let (binance_res, tv_res, yahoo_res) = future::join3(
            batch_group(&self.binance, &binance_reqs),
            batch_group(&self.tradingview, &tv_reqs),
            batch_group(&self.yahoo, &yahoo_reqs),
        )
        .await;

        let mut output: BTreeMap<CandleRequest, Vec<Candle>> = BTreeMap::new();
        output.extend(binance_res);
        output.extend(yahoo_res);

        // TV results: collect empties that have a Yahoo fallback
        let mut needs_yahoo_fb: Vec<(CandleRequest, CandleRequest)> = Vec::new();
        for (req, candles) in tv_res {
            if candles.is_empty() {
                if let Some(fb_sym) = fallback_map.get(&req) {
                    let fb_req = CandleRequest::new(fb_sym, req.timeframe, req.limit);
                    needs_yahoo_fb.push((req, fb_req));
                    continue;
                }
            }
            output.insert(req, candles);
        }

        // Batch-fetch Yahoo fallbacks
        if !needs_yahoo_fb.is_empty() {
            let fb_reqs: Vec<CandleRequest> = needs_yahoo_fb.iter().map(|(_, fb)| fb.clone()).collect();
            let fb_results = batch_group(&self.yahoo, &fb_reqs).await;
            for (orig_req, fb_req) in needs_yahoo_fb {
                output.insert(orig_req, fb_results.get(&fb_req).cloned().unwrap_or_default());
            }
        }

        Ok(output)
    }
}

// ─── ConfiguredMarketData (kept for backward compat) ──────────────────────────

#[derive(Clone)]
pub struct ConfiguredMarketData {
    primary: DataProvider,
    fallback: DataProvider,
}

#[derive(Clone)]
enum DataProvider {
    Binance(binance::BinanceDataSource),
    Yahoo(yahoo::YahooDataSource),
    TradingView(tradingview::TradingViewDataSource),
    Empty(EmptyDataSource),
}

impl ConfiguredMarketData {
    pub fn from_config(config: &AppConfig) -> Self {
        let retry = config.data_sources.retry.clone();
        let rate_limit = config.exchange.rate_limit_per_second as u32;
        let primary = provider_from_name(&config.data_sources.primary, config, rate_limit, retry.clone());
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
            Self::Binance(s) => s.candles(symbol, timeframe, limit).await,
            Self::Yahoo(s) => s.candles(symbol, timeframe, limit).await,
            Self::TradingView(s) => s.candles(symbol, timeframe, limit).await,
            Self::Empty(s) => s.candles(symbol, timeframe, limit).await,
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
        "yahoo" => DataProvider::Yahoo(yahoo::YahooDataSource::new(
            config.data_sources.yahoo.clone(),
            retry,
        )),
        "tradingview" => DataProvider::TradingView(tradingview::TradingViewDataSource::new(
            config.data_sources.tradingview.clone(),
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
        "binance" => "binance",
        "tradingview" => "tradingview",
        "yahoo" => "yahoo",
        _ => global_primary,
    }
}

fn route_source_from_str(name: &str) -> RouteSource {
    match name {
        "binance" => RouteSource::Binance,
        "tradingview" => RouteSource::TradingView,
        _ => RouteSource::Yahoo,
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
        "1m" | "M1" => Ok(Timeframe::M1),
        "5m" | "M5" => Ok(Timeframe::M5),
        "15m" | "M15" => Ok(Timeframe::M15),
        "1h" | "H1" => Ok(Timeframe::H1),
        "4h" | "H4" => Ok(Timeframe::H4),
        "1d" | "D1" => Ok(Timeframe::D1),
        "1w" | "W1" => Ok(Timeframe::W1),
        "1mo" | "MN1" | "Mn1" => Ok(Timeframe::Mn1),
        _ => Err(anyhow!("unsupported timeframe {value}")),
    }
}

pub fn binance_interval(timeframe: Timeframe) -> &'static str {
    match timeframe {
        Timeframe::M1 => "1m",
        Timeframe::M5 => "5m",
        Timeframe::M15 => "15m",
        Timeframe::H1 => "1h",
        Timeframe::H4 => "4h",
        Timeframe::D1 => "1d",
        Timeframe::W1 => "1w",
        Timeframe::Mn1 => "1M",
    }
}

pub fn yahoo_interval(timeframe: Timeframe) -> Option<&'static str> {
    match timeframe {
        Timeframe::D1 => Some("1d"),
        Timeframe::W1 => Some("1wk"),
        Timeframe::Mn1 => Some("1mo"),
        _ => None,
    }
}
