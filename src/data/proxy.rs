use std::collections::BTreeMap;

use anyhow::Result;

use crate::{config::ProxySymbols, Candle, Timeframe};

use super::MarketDataSource;

#[derive(Debug, Clone, Default)]
pub struct ProxySnapshot {
    pub xauusd: Vec<Candle>,
    pub ihsg: Vec<Candle>,
    pub dxy: Vec<Candle>,
}

pub async fn fetch_once_per_cycle(
    source: &dyn MarketDataSource,
    proxies: &ProxySymbols,
    timeframe: Timeframe,
    limit: usize,
) -> Result<ProxySnapshot> {
    let symbols = BTreeMap::from([
        ("xauusd", proxies.xauusd.as_str()),
        ("ihsg", proxies.ihsg.as_str()),
        ("dxy", proxies.dxy.as_str()),
    ]);

    let xauusd = source.candles(symbols["xauusd"], timeframe, limit).await?;
    let ihsg = source.candles(symbols["ihsg"], timeframe, limit).await?;
    let dxy = source.candles(symbols["dxy"], timeframe, limit).await?;

    Ok(ProxySnapshot { xauusd, ihsg, dxy })
}
