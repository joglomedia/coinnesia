use anyhow::Result;
use async_trait::async_trait;

use crate::{data::MarketDataSource, Candle, Timeframe};

#[derive(Debug, Clone, Default)]
pub struct YahooDataSource;

#[async_trait]
impl MarketDataSource for YahooDataSource {
    async fn candles(
        &self,
        _symbol: &str,
        _timeframe: Timeframe,
        _limit: usize,
    ) -> Result<Vec<Candle>> {
        Ok(Vec::new())
    }
}
