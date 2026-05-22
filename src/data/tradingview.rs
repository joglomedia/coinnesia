use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, OnceLock},
};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use tvdata_rs::prelude::{
    AuthConfig, Bar, HistoryRequest, Interval, Ticker, TradingSession, TradingViewClient,
    TradingViewClientConfig,
};

use crate::{
    config::{RetryConfig, TradingViewDataSourceConfig},
    data::{retry, CandleRequest, MarketDataSource},
    Candle, Timeframe,
};

#[derive(Clone)]
pub struct TradingViewDataSource {
    config: TradingViewDataSourceConfig,
    retry: RetryConfig,
    client: Arc<OnceLock<Result<Arc<TradingViewClient>, String>>>,
}

impl fmt::Debug for TradingViewDataSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TradingViewDataSource")
            .field("enabled", &self.config.enabled)
            .field("auth_token_env", &self.config.auth_token_env)
            .field("session_id_env", &self.config.session_id_env)
            .field("session_signature_env", &self.config.session_signature_env)
            .field("device_token_env", &self.config.device_token_env)
            .finish_non_exhaustive()
    }
}

impl TradingViewDataSource {
    pub fn new(config: TradingViewDataSourceConfig, retry_config: RetryConfig) -> Self {
        Self {
            config,
            retry: retry_config,
            client: Arc::new(OnceLock::new()),
        }
    }

    pub fn auth_available(&self) -> bool {
        self.config.enabled
            && (env_value(&self.config.auth_token_env).is_some()
                || env_value(&self.config.session_id_env).is_some())
    }

    fn client(&self) -> Result<Arc<TradingViewClient>> {
        let result = self.client.get_or_init(|| {
            build_client(&self.config)
                .map(Arc::new)
                .map_err(|error| error.to_string())
        });
        result
            .as_ref()
            .map(Arc::clone)
            .map_err(|error| anyhow!("failed to initialize TradingView client: {error}"))
    }
}

impl Default for TradingViewDataSource {
    fn default() -> Self {
        Self::new(
            TradingViewDataSourceConfig {
                enabled: false,
                auth_token_env: "TRADINGVIEW_AUTH_TOKEN".to_owned(),
                session_id_env: "TRADINGVIEW_SESSION_ID".to_owned(),
                session_signature_env: "TRADINGVIEW_SESSIONID_SIGN".to_owned(),
                device_token_env: "TRADINGVIEW_DEVICE_T".to_owned(),
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
impl MarketDataSource for TradingViewDataSource {
    async fn candles(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<Candle>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let client = self.client()?;
        let tv_symbol = normalize_symbol(symbol);
        let retry_config = self.retry.clone();
        let tv_sym = tv_symbol.clone();
        let series = retry::with_retry(&retry_config, || {
            let client = client.clone();
            let tv_sym = tv_sym.clone();
            async move {
                client
                    .history(
                        &HistoryRequest::new(
                            tv_sym.clone(),
                            tradingview_interval(timeframe),
                            bars(limit),
                        )
                        .session(TradingSession::Regular),
                    )
                    .await
                    .with_context(|| {
                        format!(
                            "failed to fetch TradingView candles for {tv_sym} {:?}",
                            timeframe
                        )
                    })
            }
        })
        .await?;

        series.bars.into_iter().map(candle_from_bar).collect()
    }

    async fn batch_candles(
        &self,
        requests: &[CandleRequest],
    ) -> Result<BTreeMap<CandleRequest, Vec<Candle>>> {
        let mut output = BTreeMap::new();
        if requests.is_empty() {
            return Ok(output);
        }
        if !self.config.enabled {
            output.extend(
                requests
                    .iter()
                    .cloned()
                    .map(|request| (request, Vec::new())),
            );
            return Ok(output);
        }

        let client = self.client()?;
        let mut groups = BTreeMap::<(Timeframe, usize), Vec<CandleRequest>>::new();
        for request in requests {
            groups
                .entry((request.timeframe, request.limit))
                .or_default()
                .push(request.clone());
        }

        for ((timeframe, limit), group) in groups {
            let symbols = group
                .iter()
                .map(|request| Ticker::new(normalize_symbol(&request.symbol)))
                .collect::<Vec<_>>();
            let retry_config = self.retry.clone();
            let series = retry::with_retry(&retry_config, || {
                let client = client.clone();
                let syms = symbols.clone();
                async move {
                    client
                        .download_history_map(syms, tradingview_interval(timeframe), bars(limit))
                        .await
                        .with_context(|| {
                            format!(
                                "failed to fetch TradingView batch candles for {:?} limit {}",
                                timeframe, limit
                            )
                        })
                }
            })
            .await?;

            for request in group {
                let tv_symbol = normalize_symbol(&request.symbol);
                let candles = series
                    .iter()
                    .find(|(symbol, _)| symbol.as_str() == tv_symbol)
                    .map(|(_, series)| {
                        series
                            .bars
                            .iter()
                            .cloned()
                            .map(candle_from_bar)
                            .collect::<Result<Vec<_>>>()
                    })
                    .transpose()?
                    .unwrap_or_default();
                output.insert(request, candles);
            }
        }

        Ok(output)
    }
}

fn build_client(config: &TradingViewDataSourceConfig) -> Result<TradingViewClient> {
    let mut client_config = TradingViewClientConfig::backend_history();
    client_config.auth = auth_config(config);
    TradingViewClient::from_config(client_config).context("invalid tvdata-rs client config")
}

fn auth_config(config: &TradingViewDataSourceConfig) -> AuthConfig {
    let auth_token = env_value(&config.auth_token_env);
    let session_id = env_value(&config.session_id_env);

    match (session_id, auth_token) {
        (Some(session_id), Some(auth_token)) => {
            AuthConfig::session_and_token(session_id, auth_token)
        }
        (Some(session_id), None) => AuthConfig::session(session_id),
        (None, Some(auth_token)) => AuthConfig::token(auth_token),
        (None, None) => AuthConfig::anonymous(),
    }
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn normalize_symbol(symbol: &str) -> String {
    if symbol.contains(':') {
        return symbol.to_owned();
    }

    if symbol.ends_with("USDT") || symbol.ends_with("BUSD") || symbol.ends_with("USDC") {
        return format!("BINANCE:{symbol}");
    }

    symbol.to_owned()
}

fn tradingview_interval(timeframe: Timeframe) -> Interval {
    match timeframe {
        Timeframe::M1 => Interval::Min1,
        Timeframe::M5 => Interval::Min5,
        Timeframe::M15 => Interval::Min15,
        Timeframe::H1 => Interval::Hour1,
        Timeframe::H4 => Interval::Hour4,
        Timeframe::D1 => Interval::Day1,
        Timeframe::W1 => Interval::Week1,
        Timeframe::Mn1 => Interval::Month1,
    }
}

fn bars(limit: usize) -> u32 {
    limit.clamp(1, 5_000) as u32
}

fn candle_from_bar(bar: Bar) -> Result<Candle> {
    Ok(Candle {
        ts: Utc
            .timestamp_opt(bar.time.unix_timestamp(), bar.time.nanosecond())
            .single()
            .context("invalid TradingView bar timestamp")?,
        open: bar.open,
        high: bar.high,
        low: bar.low,
        close: bar.close,
        volume: bar.volume.unwrap_or(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tradingview_config_supports_optional_auth_envs() {
        let source = TradingViewDataSource::new(
            TradingViewDataSourceConfig {
                enabled: true,
                auth_token_env: "COINNESIA_TEST_TV_AUTH_TOKEN_MISSING".to_owned(),
                session_id_env: "COINNESIA_TEST_TV_SESSION_ID_MISSING".to_owned(),
                session_signature_env: "COINNESIA_TEST_TV_SESSION_SIGN_MISSING".to_owned(),
                device_token_env: "COINNESIA_TEST_TV_DEVICE_T_MISSING".to_owned(),
            },
            RetryConfig {
                max_retries: 3,
                base_delay_ms: 500,
                max_delay_ms: 10000,
            },
        );

        assert!(!source.auth_available());
        assert!(source.client().is_ok());
    }

    #[test]
    fn normalizes_crypto_symbols_for_tradingview_history() {
        assert_eq!(normalize_symbol("BTCUSDT"), "BINANCE:BTCUSDT");
        assert_eq!(normalize_symbol("FX:EURUSD"), "FX:EURUSD");
    }

    #[test]
    fn maps_internal_timeframes_to_tradingview_intervals() {
        assert_eq!(tradingview_interval(Timeframe::M1), Interval::Min1);
        assert_eq!(tradingview_interval(Timeframe::H4), Interval::Hour4);
        assert_eq!(tradingview_interval(Timeframe::Mn1), Interval::Month1);
    }
}
