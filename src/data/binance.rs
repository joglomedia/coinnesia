use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use reqwest::Url;
use serde_json::Value;

use crate::{
    config::{BinanceConfig, RetryConfig},
    data::{binance_interval, retry, MarketDataSource},
    Candle, Timeframe,
};

#[derive(Clone)]
pub struct BinanceDataSource {
    client: reqwest::Client,
    config: BinanceConfig,
    rate_limiter: Arc<retry::RateLimiter>,
    retry: RetryConfig,
}

impl std::fmt::Debug for BinanceDataSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BinanceDataSource")
            .field("rest_url", &self.config.rest_url)
            .field("testnet", &self.config.testnet)
            .finish_non_exhaustive()
    }
}

impl BinanceDataSource {
    pub fn new(config: BinanceConfig, rate_limit_per_second: u32, retry_config: RetryConfig) -> Self {
        let rate_limit = rate_limit_per_second.max(1);
        Self {
            client: reqwest::Client::new(),
            rate_limiter: Arc::new(retry::RateLimiter::new(
                rate_limit,
                Duration::from_secs(1),
            )),
            config,
            retry: retry_config,
        }
    }

    pub fn credentials(&self) -> BinanceCredentials {
        BinanceCredentials {
            api_key: non_empty(self.config.api_key.clone())
                .or_else(|| std::env::var(&self.config.api_key_env).ok()),
            api_secret: non_empty(self.config.api_secret.clone())
                .or_else(|| std::env::var(&self.config.api_secret_env).ok()),
        }
    }

    fn klines_url(&self, symbol: &str, timeframe: Timeframe, limit: usize) -> Result<Url> {
        let mut url = Url::parse(&self.config.rest_url)
            .context("invalid Binance REST URL")?
            .join("/api/v3/klines")
            .context("invalid Binance klines endpoint")?;
        url.query_pairs_mut()
            .append_pair("symbol", symbol)
            .append_pair("interval", binance_interval(timeframe))
            .append_pair("limit", &limit.min(1000).to_string());
        Ok(url)
    }
}

impl Default for BinanceDataSource {
    fn default() -> Self {
        Self::new(
            BinanceConfig {
                api_key: None,
                api_secret: None,
                api_key_env: "BINANCE_API_KEY".to_owned(),
                api_secret_env: "BINANCE_API_SECRET".to_owned(),
                account_type: "spot".to_owned(),
                recv_window: 5,
                testnet: false,
                market_data_mode: "http".to_owned(),
                http_poll_interval: 1,
                rest_url: "https://api.binance.com".to_owned(),
                websocket_url: "wss://stream.binance.com:9443".to_owned(),
                ws: crate::config::BinanceWsConfig {
                    enabled: false,
                    url: "wss://stream.binance.com/stream".to_owned(),
                    max_streams_per_connection: 200,
                    reconnect_base_delay_ms: 1000,
                    reconnect_max_delay_ms: 30000,
                    candle_buffer_size: 500,
                },
            },
            10,
            RetryConfig {
                max_retries: 3,
                base_delay_ms: 500,
                max_delay_ms: 10000,
            },
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinanceCredentials {
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
}

#[async_trait]
impl MarketDataSource for BinanceDataSource {
    async fn candles(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<Candle>> {
        self.rate_limiter.acquire().await;

        let url = self.klines_url(symbol, timeframe, limit)?;
        let client = self.client.clone();
        let rows = retry::with_retry(&self.retry, || {
            let client = client.clone();
            let url = url.clone();
            async move {
                client
                    .get(url)
                    .send()
                    .await
                    .context("failed to request Binance klines")?
                    .error_for_status()
                    .context("Binance klines request returned error status")?
                    .json::<Vec<Vec<Value>>>()
                    .await
                    .context("failed to decode Binance klines")
            }
        })
        .await?;

        rows.into_iter().map(parse_kline).collect()
    }
}

fn parse_kline(row: Vec<Value>) -> Result<Candle> {
    let ts = row
        .first()
        .and_then(Value::as_i64)
        .context("missing Binance kline open time")?;
    Ok(Candle {
        ts: Utc
            .timestamp_millis_opt(ts)
            .single()
            .context("invalid Binance kline timestamp")?,
        open: parse_decimal_string(&row, 1, "open")?,
        high: parse_decimal_string(&row, 2, "high")?,
        low: parse_decimal_string(&row, 3, "low")?,
        close: parse_decimal_string(&row, 4, "close")?,
        volume: parse_decimal_string(&row, 5, "volume")?,
    })
}

fn parse_decimal_string(row: &[Value], index: usize, field: &str) -> Result<f64> {
    row.get(index)
        .and_then(Value::as_str)
        .with_context(|| format!("missing Binance kline {field}"))?
        .parse::<f64>()
        .with_context(|| format!("invalid Binance kline {field}"))
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}
