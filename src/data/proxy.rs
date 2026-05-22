use anyhow::Result;

use crate::{config::ProxySymbols, Candle, Timeframe};

use super::{CandleRequest, MarketDataSource};

#[derive(Debug, Clone, Default)]
pub struct ProxySnapshot {
    pub xauusd: Vec<Candle>,
    pub ihsg: Vec<Candle>,
    pub dxy: Vec<Candle>,
}

/// Fetch proxy symbols once per scan cycle.
///
/// Uses each `ProxySymbolEntry.symbol()` as the request key — this returns the
/// preferred-source symbol (e.g. "OANDA:XAUUSD" for TradingView, "GC=F" for Yahoo).
/// When `source` is `PerSymbolMarketData`, routing and Yahoo fallback are handled
/// transparently inside the adapter.
pub async fn fetch_once_per_cycle(
    source: &dyn MarketDataSource,
    proxies: &ProxySymbols,
    timeframe: Timeframe,
    limit: usize,
) -> Result<ProxySnapshot> {
    let xauusd_req = CandleRequest::new(proxies.xauusd.symbol(), timeframe, limit);
    let ihsg_req = CandleRequest::new(proxies.ihsg.symbol(), timeframe, limit);
    let dxy_req = CandleRequest::new(proxies.dxy.symbol(), timeframe, limit);

    let candles = source
        .batch_candles(&[xauusd_req.clone(), ihsg_req.clone(), dxy_req.clone()])
        .await?;

    Ok(ProxySnapshot {
        xauusd: candles.get(&xauusd_req).cloned().unwrap_or_default(),
        ihsg: candles.get(&ihsg_req).cloned().unwrap_or_default(),
        dxy: candles.get(&dxy_req).cloned().unwrap_or_default(),
    })
}
