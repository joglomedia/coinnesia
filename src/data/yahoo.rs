use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use reqwest::Url;
use serde_json::Value;

use crate::{
    config::{RetryConfig, YahooDataSourceConfig},
    data::{retry, yahoo_interval, MarketDataSource},
    Candle, Timeframe,
};

#[derive(Debug, Clone)]
pub struct YahooDataSource {
    client: reqwest::Client,
    config: YahooDataSourceConfig,
    retry: RetryConfig,
}

impl YahooDataSource {
    pub fn new(config: YahooDataSourceConfig, retry_config: RetryConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
            retry: retry_config,
        }
    }

    fn chart_url(&self, symbol: &str, timeframe: Timeframe, limit: usize) -> Result<Url> {
        let interval = yahoo_interval(timeframe)
            .ok_or_else(|| anyhow!("Yahoo fallback only supports daily or higher intervals"))?;
        let range = match timeframe {
            Timeframe::D1 => "1y",
            Timeframe::W1 => "5y",
            Timeframe::Mn1 => "10y",
            _ => "1y",
        };
        let mut url = Url::parse(&self.config.base_url)
            .context("invalid Yahoo base URL")?
            .join(&format!("/v8/finance/chart/{symbol}"))
            .context("invalid Yahoo chart endpoint")?;
        url.query_pairs_mut()
            .append_pair("interval", interval)
            .append_pair("range", range)
            .append_pair("includePrePost", "false")
            .append_pair(
                "events",
                if self.config.adjust {
                    "div,splits"
                } else {
                    "history"
                },
            )
            .append_pair("limit", &limit.to_string());
        Ok(url)
    }
}

impl Default for YahooDataSource {
    fn default() -> Self {
        Self::new(
            YahooDataSourceConfig {
                enabled: true,
                base_url: "https://query1.finance.yahoo.com".to_owned(),
                adjust: true,
            },
            RetryConfig {
                max_retries: 3,
                base_delay_ms: 500,
                max_delay_ms: 10000,
            },
        )
    }
}

#[async_trait]
impl MarketDataSource for YahooDataSource {
    async fn candles(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<Candle>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let url = self.chart_url(symbol, timeframe, limit)?;
        let client = self.client.clone();
        let body = retry::with_retry(&self.retry, || {
            let client = client.clone();
            let url = url.clone();
            async move {
                client
                    .get(url)
                    .send()
                    .await
                    .context("failed to request Yahoo chart")?
                    .error_for_status()
                    .context("Yahoo chart request returned error status")?
                    .json::<Value>()
                    .await
                    .context("failed to decode Yahoo chart")
            }
        })
        .await?;

        parse_chart(body, limit)
    }
}

fn parse_chart(body: Value, limit: usize) -> Result<Vec<Candle>> {
    let result = body
        .pointer("/chart/result/0")
        .context("missing Yahoo chart result")?;
    let timestamps = result
        .get("timestamp")
        .and_then(Value::as_array)
        .context("missing Yahoo timestamps")?;
    let quote = result
        .pointer("/indicators/quote/0")
        .context("missing Yahoo quote payload")?;
    let opens = array(quote, "open")?;
    let highs = array(quote, "high")?;
    let lows = array(quote, "low")?;
    let closes = array(quote, "close")?;
    let volumes = array(quote, "volume")?;

    let mut candles = Vec::new();
    for idx in 0..timestamps.len() {
        let Some(ts) = timestamps[idx].as_i64() else {
            continue;
        };
        let Some(open) = number(opens, idx) else {
            continue;
        };
        let Some(high) = number(highs, idx) else {
            continue;
        };
        let Some(low) = number(lows, idx) else {
            continue;
        };
        let Some(close) = number(closes, idx) else {
            continue;
        };
        let volume = number(volumes, idx).unwrap_or(0.0);
        candles.push(Candle {
            ts: Utc
                .timestamp_opt(ts, 0)
                .single()
                .context("invalid Yahoo timestamp")?,
            open,
            high,
            low,
            close,
            volume,
        });
    }

    let start = candles.len().saturating_sub(limit);
    Ok(candles.split_off(start))
}

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    value
        .get(field)
        .and_then(Value::as_array)
        .with_context(|| format!("missing Yahoo {field} array"))
}

fn number(values: &[Value], index: usize) -> Option<f64> {
    values.get(index).and_then(Value::as_f64)
}
