use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
use reqwest::Url;
use serde::Deserialize;

use crate::{
    config::{RetryConfig, TwelveDataDataSourceConfig},
    data::{retry, MarketDataSource},
    Candle, Timeframe,
};

#[derive(Debug, Clone)]
pub struct TwelveDataDataSource {
    client: reqwest::Client,
    config: TwelveDataDataSourceConfig,
    retry: RetryConfig,
}

impl TwelveDataDataSource {
    pub fn new(config: TwelveDataDataSourceConfig, retry_config: RetryConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
            retry: retry_config,
        }
    }

    fn api_key(&self) -> Result<String> {
        std::env::var(&self.config.api_key_env).with_context(|| {
            format!(
                "Twelve Data API key not set (env var: {}). \
                 Get a free key at https://twelvedata.com/pricing",
                self.config.api_key_env
            )
        })
    }

    fn time_series_url(&self, symbol: &str, timeframe: Timeframe, limit: usize) -> Result<Url> {
        let interval = twelvedata_interval(timeframe)
            .ok_or_else(|| anyhow!("Twelve Data does not support timeframe {timeframe:?}"))?;
        let api_key = self.api_key()?;
        let mut url = Url::parse(&self.config.base_url)
            .context("invalid Twelve Data base URL")?
            .join("/time_series")
            .context("invalid Twelve Data time_series endpoint")?;
        url.query_pairs_mut()
            .append_pair("symbol", symbol)
            .append_pair("interval", interval)
            .append_pair("outputsize", &limit.min(5000).to_string())
            .append_pair("order", "asc")
            .append_pair("apikey", &api_key);
        Ok(url)
    }
}

#[async_trait]
impl MarketDataSource for TwelveDataDataSource {
    async fn candles(&self, symbol: &str, timeframe: Timeframe, limit: usize) -> Result<Vec<Candle>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let url = self.time_series_url(symbol, timeframe, limit)?;
        let client = self.client.clone();
        let body = retry::with_retry(&self.retry, || {
            let client = client.clone();
            let url = url.clone();
            async move {
                client
                    .get(url)
                    .send()
                    .await
                    .context("failed to request Twelve Data time_series")?
                    .error_for_status()
                    .context("Twelve Data time_series request returned error status")?
                    .json::<TdResponse>()
                    .await
                    .context("failed to decode Twelve Data response")
            }
        })
        .await?;

        if body.status != "ok" {
            return Err(anyhow!(
                "Twelve Data error {}: {}",
                body.code.unwrap_or(0),
                body.message.as_deref().unwrap_or("unknown error")
            ));
        }

        body.values
            .unwrap_or_default()
            .into_iter()
            .map(parse_bar)
            .collect()
    }
}

fn twelvedata_interval(timeframe: Timeframe) -> Option<&'static str> {
    Some(match timeframe {
        Timeframe::M1  => "1min",
        Timeframe::M5  => "5min",
        Timeframe::M15 => "15min",
        Timeframe::H1  => "1h",
        Timeframe::H4  => "4h",
        Timeframe::D1  => "1day",
        Timeframe::W1  => "1week",
        Timeframe::Mn1 => "1month",
    })
}

fn parse_bar(bar: TdBar) -> Result<Candle> {
    Ok(Candle {
        ts: parse_datetime(&bar.datetime)?,
        open:   bar.open.parse::<f64>().with_context(|| format!("invalid open: {}", bar.open))?,
        high:   bar.high.parse::<f64>().with_context(|| format!("invalid high: {}", bar.high))?,
        low:    bar.low.parse::<f64>().with_context(|| format!("invalid low: {}", bar.low))?,
        close:  bar.close.parse::<f64>().with_context(|| format!("invalid close: {}", bar.close))?,
        volume: bar.volume.as_deref().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
    })
}

fn parse_datetime(s: &str) -> Result<chrono::DateTime<Utc>> {
    // Daily format: "2026-05-22"
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(Utc.from_utc_datetime(
            &d.and_hms_opt(0, 0, 0).context("invalid date components")?,
        ));
    }
    // Intraday format: "2026-05-22 09:30:00"
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .with_context(|| format!("invalid Twelve Data datetime: {s}"))
        .map(|dt| Utc.from_utc_datetime(&dt))
}

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TdResponse {
    status: String,
    values: Option<Vec<TdBar>>,
    message: Option<String>,
    code: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TdBar {
    datetime: String,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: Option<String>,
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn ok_response_json() -> &'static str {
        r#"{
            "meta": {"symbol": "XAU/USD", "interval": "1day"},
            "values": [
                {
                    "datetime": "2026-05-20",
                    "open":  "3233.01000",
                    "high":  "3250.00000",
                    "low":   "3200.50000",
                    "close": "3241.75000",
                    "volume": "0"
                },
                {
                    "datetime": "2026-05-21",
                    "open":  "3241.75000",
                    "high":  "3260.00000",
                    "low":   "3225.00000",
                    "close": "3255.10000"
                }
            ],
            "status": "ok"
        }"#
    }

    fn error_response_json() -> &'static str {
        r#"{"code": 401, "message": "Invalid API key", "status": "error"}"#
    }

    #[test]
    fn parse_bar_produces_correct_candle() {
        let response: TdResponse = serde_json::from_str(ok_response_json()).unwrap();
        assert_eq!(response.status, "ok");
        let bars = response.values.unwrap();
        assert_eq!(bars.len(), 2);

        let candle = parse_bar(bars.into_iter().next().unwrap()).unwrap();
        assert_eq!(candle.ts.to_rfc3339(), "2026-05-20T00:00:00+00:00");
        assert_relative_eq!(candle.open,  3233.01, epsilon = 1e-4);
        assert_relative_eq!(candle.high,  3250.00, epsilon = 1e-4);
        assert_relative_eq!(candle.low,   3200.50, epsilon = 1e-4);
        assert_relative_eq!(candle.close, 3241.75, epsilon = 1e-4);
        assert_relative_eq!(candle.volume, 0.0, epsilon = 1e-6);
    }

    #[test]
    fn parse_bar_handles_missing_volume() {
        let response: TdResponse = serde_json::from_str(ok_response_json()).unwrap();
        let bars = response.values.unwrap();
        // Second bar has no "volume" field
        let candle = parse_bar(bars.into_iter().nth(1).unwrap()).unwrap();
        assert_relative_eq!(candle.volume, 0.0, epsilon = 1e-6);
    }

    #[test]
    fn parse_error_response_returns_err() {
        let response: TdResponse = serde_json::from_str(error_response_json()).unwrap();
        assert_eq!(response.status, "error");
        assert_eq!(response.code.unwrap(), 401);
        assert_eq!(response.message.as_deref().unwrap(), "Invalid API key");
    }

    #[test]
    fn parse_datetime_handles_daily_format() {
        let dt = parse_datetime("2026-05-22").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-05-22T00:00:00+00:00");
    }

    #[test]
    fn parse_datetime_handles_intraday_format() {
        let dt = parse_datetime("2026-05-22 09:30:00").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-05-22T09:30:00+00:00");
    }

    #[test]
    fn twelvedata_interval_covers_all_timeframes() {
        use Timeframe::*;
        for tf in [M1, M5, M15, H1, H4, D1, W1, Mn1] {
            assert!(twelvedata_interval(tf).is_some(), "missing interval for {tf:?}");
        }
    }

    /// Live smoke test — requires TWELVE_DATA_API_KEY in environment.
    /// Run with: cargo test twelvedata_smoke_xauusd -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn twelvedata_smoke_xauusd() {
        let source = TwelveDataDataSource::new(
            TwelveDataDataSourceConfig {
                enabled: true,
                base_url: "https://api.twelvedata.com".to_owned(),
                api_key_env: "TWELVE_DATA_API_KEY".to_owned(),
            },
            RetryConfig { max_retries: 1, base_delay_ms: 500, max_delay_ms: 5000 },
        );
        let candles = source.candles("XAU/USD", Timeframe::D1, 5).await.unwrap();
        assert!(!candles.is_empty(), "expected at least one candle for XAU/USD");
        for c in &candles {
            assert!(c.open > 0.0, "open > 0");
            assert!(c.high >= c.low, "high >= low");
        }
        let ascending = candles.windows(2).all(|w| w[0].ts <= w[1].ts);
        assert!(ascending, "candles should be in ascending order");
        println!("XAU/USD D1 (5 bars): {candles:#?}");
    }
}
