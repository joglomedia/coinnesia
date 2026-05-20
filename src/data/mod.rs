pub mod binance;
pub mod proxy;
pub mod tradingview;
pub mod yahoo;

use anyhow::Result;
use async_trait::async_trait;

use crate::{Candle, Timeframe};

#[async_trait]
pub trait MarketDataSource: Send + Sync {
    async fn candles(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<Candle>>;
}

#[derive(Debug, Clone, Default)]
pub struct EmptyDataSource;

#[async_trait]
impl MarketDataSource for EmptyDataSource {
    async fn candles(
        &self,
        _symbol: &str,
        _timeframe: Timeframe,
        _limit: usize,
    ) -> Result<Vec<Candle>> {
        Ok(Vec::new())
    }
}
