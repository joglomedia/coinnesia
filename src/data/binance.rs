use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use async_trait::async_trait;
use binance_sdk::{
    config::ConfigurationRestApi,
    spot::{
        rest_api::{KlinesIntervalEnum, KlinesItemInner, KlinesParams},
        SpotRestApi,
    },
};
use chrono::{TimeZone, Utc};

use crate::{
    config::{BinanceConfig, RetryConfig},
    data::{retry, MarketDataSource},
    Candle, Timeframe,
};

#[derive(Clone)]
pub struct BinanceDataSource {
    api: binance_sdk::spot::rest_api::RestApi,
    rate_limiter: Arc<retry::RateLimiter>,
    retry: RetryConfig,
}

impl std::fmt::Debug for BinanceDataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BinanceDataSource")
            .finish_non_exhaustive()
    }
}

impl BinanceDataSource {
    pub fn new(config: BinanceConfig, rate_limit_per_second: u32, retry_config: RetryConfig) -> Self {
        let api = SpotRestApi::from_config(build_sdk_config(&config));
        Self {
            api,
            rate_limiter: Arc::new(retry::RateLimiter::new(
                rate_limit_per_second.max(1),
                Duration::from_secs(1),
            )),
            retry: retry_config,
        }
    }
}

#[async_trait]
impl MarketDataSource for BinanceDataSource {
    async fn candles(&self, symbol: &str, timeframe: Timeframe, limit: usize) -> Result<Vec<Candle>> {
        self.rate_limiter.acquire().await;

        let params = KlinesParams::builder(symbol.to_uppercase(), timeframe_to_sdk_interval(timeframe)?)
            .limit(limit.min(1000) as i32)
            .build()
            .context("build Binance klines params")?;

        let api = self.api.clone();
        let rows: Vec<Vec<KlinesItemInner>> = retry::with_retry(&self.retry, || {
            let api    = api.clone();
            let params = params.clone();
            async move {
                api.klines(params)
                    .await
                    .context("Binance klines SDK request")?
                    .data()
                    .await
                    .context("Binance klines deserialization")
            }
        })
        .await?;

        rows.into_iter().map(parse_kline).collect()
    }
}

// ─── SDK helpers ─────────────────────────────────────────────────────────────

fn build_sdk_config(cfg: &BinanceConfig) -> ConfigurationRestApi {
    ConfigurationRestApi::builder()
        .base_path(cfg.rest_url.clone())
        // Set to 1 (not 0) to avoid SDK underflow: it evaluates `retries - attempt`
        // before the retry-guard check. With retries=1 and attempt=1, the result is 0
        // which makes should_retry_request return false — effectively no SDK retry.
        // Actual retry logic is handled by our retry::with_retry wrapper.
        .retries(1u32)
        // SDK default is 1000ms — way too short for SEA → Binance latency (~200-400ms).
        // Match the server.request_timeout_secs default from AppConfig (10s).
        .timeout(10_000u64)
        .build()
        .expect("BinanceConfig always produces a valid SDK configuration")
}

fn timeframe_to_sdk_interval(tf: Timeframe) -> Result<KlinesIntervalEnum> {
    Ok(match tf {
        Timeframe::M1  => KlinesIntervalEnum::Interval1m,
        Timeframe::M5  => KlinesIntervalEnum::Interval5m,
        Timeframe::M15 => KlinesIntervalEnum::Interval15m,
        Timeframe::H1  => KlinesIntervalEnum::Interval1h,
        Timeframe::H4  => KlinesIntervalEnum::Interval4h,
        Timeframe::D1  => KlinesIntervalEnum::Interval1d,
        Timeframe::W1  => KlinesIntervalEnum::Interval1w,
        Timeframe::Mn1 => KlinesIntervalEnum::Interval1M,
    })
}

fn parse_kline(row: Vec<KlinesItemInner>) -> Result<Candle> {
    let ts_ms = match row.first().context("empty kline row")? {
        KlinesItemInner::Integer(ms) => *ms,
        other => anyhow::bail!("unexpected kline open_time type: {other:?}"),
    };
    Ok(Candle {
        ts: Utc
            .timestamp_millis_opt(ts_ms)
            .single()
            .context("invalid Binance kline timestamp")?,
        open:   parse_price(&row, 1, "open")?,
        high:   parse_price(&row, 2, "high")?,
        low:    parse_price(&row, 3, "low")?,
        close:  parse_price(&row, 4, "close")?,
        volume: parse_price(&row, 5, "volume")?,
    })
}

fn parse_price(row: &[KlinesItemInner], idx: usize, field: &str) -> Result<f64> {
    match row.get(idx).with_context(|| format!("missing kline field '{field}'"))? {
        KlinesItemInner::String(s) => {
            s.parse::<f64>().with_context(|| format!("invalid kline '{field}': {s}"))
        }
        other => anyhow::bail!("unexpected type for kline '{field}': {other:?}"),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kline_sdk_parity() {
        let raw = serde_json::json!([
            1682899200000i64,
            "29000.00", "29500.00", "28800.00", "29300.00", "1234.567",
            1682902799999i64, "36000000.00", 1000, "600.000", "17000000.00", "0"
        ]);
        let row: Vec<KlinesItemInner> = serde_json::from_value(raw).unwrap();
        let candle = parse_kline(row).unwrap();
        assert_eq!(candle.open,  29000.0);
        assert_eq!(candle.high,  29500.0);
        assert_eq!(candle.low,   28800.0);
        assert_eq!(candle.close, 29300.0);
        approx::assert_relative_eq!(candle.volume, 1234.567, epsilon = 1e-6);
        assert_eq!(candle.ts.timestamp_millis(), 1682899200000);
    }
}
