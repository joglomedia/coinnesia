pub mod defaults;
pub mod profiles;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

use crate::AssetClass;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub indicators: IndicatorConfig,
    pub strategy: StrategyConfig,
    pub entry_plan: EntryPlanConfig,
    pub trap_guard: TrapGuardConfig,
    pub session: SessionConfig,
    pub server: ServerConfig,
    pub alerts: AlertsConfig,
    pub database: DatabaseConfig,
    pub cache: CacheConfig,
    pub runtime: RuntimeConfig,
    pub data_sources: DataSourcesConfig,
    pub exchange: ExchangeConfig,
    pub trading: TradingConfig,
    pub portfolio: PortfolioConfig,
    pub risk: RiskConfig,
    pub backtest: BacktestConfig,
    pub symbols: Vec<SymbolConfig>,
    pub proxy_symbols: ProxySymbols,
}

impl AppConfig {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn from_default_toml() -> Result<Self> {
        toml::from_str(defaults::DEFAULT_TOML).context("failed to parse embedded default config")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorConfig {
    pub ema_fast: usize,
    pub ema_slow: usize,
    pub ema_trend: usize,
    pub atr_length: usize,
    pub rsi_length: usize,
    pub volume_ma_length: usize,
    pub adx_length: usize,
    pub adx_smoothing: usize,
    pub macd_fast: usize,
    pub macd_slow: usize,
    pub macd_signal: usize,
    #[serde(default = "default_ob_displacement_atr")]
    pub ob_displacement_atr: f64,
    #[serde(default = "default_sr_cluster_atr")]
    pub sr_cluster_atr: f64,
}

fn default_ob_displacement_atr() -> f64 {
    0.65
}

fn default_sr_cluster_atr() -> f64 {
    0.35
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub min_directional_gap: f64,
    pub min_confidence_15m: f64,
    pub min_confidence_1h: f64,
    pub min_confidence_4h: f64,
    pub min_confidence_1d: f64,
    pub structure_lookback: usize,
    pub min_structure_score: f64,
    #[serde(default)]
    pub mtf: MtfConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MtfConfig {
    /// V61.7 consensus gap threshold — block emission when |long_votes − short_votes|
    /// across M1/M5/M15 exceeds this score on the opposite side.
    #[serde(default = "default_consensus_score_gap")]
    pub consensus_score_gap: f64,
    /// V61.7 trend-flip confirm bars.
    #[serde(default = "default_flip_confirm_bars")]
    pub flip_confirm_bars: usize,
    /// V61.6 microTrend lookback (bars on lowest TF).
    #[serde(default = "default_micro_trend_bars")]
    pub micro_trend_bars: usize,
    /// V61.6 minimum directional bars within micro_trend_bars to trigger override.
    #[serde(default = "default_micro_trend_min_bars")]
    pub micro_trend_min_bars: usize,
}

impl Default for MtfConfig {
    fn default() -> Self {
        Self {
            consensus_score_gap: default_consensus_score_gap(),
            flip_confirm_bars: default_flip_confirm_bars(),
            micro_trend_bars: default_micro_trend_bars(),
            micro_trend_min_bars: default_micro_trend_min_bars(),
        }
    }
}

fn default_consensus_score_gap() -> f64 {
    12.0
}

fn default_flip_confirm_bars() -> usize {
    2
}

fn default_micro_trend_bars() -> usize {
    5
}

fn default_micro_trend_min_bars() -> usize {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryPlanConfig {
    pub swing_lookback: usize,
    pub ew1_min_atr: f64,
    pub ew1_max_atr: f64,
    pub ew2_atr: f64,
    pub ew3_atr: f64,
    pub deep_add_atr: f64,
    pub entry_zone_atr: f64,
    pub tp1_atr: f64,
    pub tp2_atr: f64,
    pub tp3_atr: f64,
    pub tp_step_min_atr: f64,
    pub max_tp1_atr: f64,
    pub max_tp2_atr: f64,
    pub max_tp3_atr: f64,
    pub min_sl_distance_atr: f64,
    pub max_sl_distance_atr: f64,
    pub sl_extra_asia_atr: f64,
    pub sl_extra_europe_atr: f64,
    pub sl_extra_usa_atr: f64,
    #[serde(default = "default_sl_trap_extra_atr")]
    pub sl_trap_extra_atr: f64,
    #[serde(default = "default_sl_wick_extra_atr")]
    pub sl_wick_extra_atr: f64,
    #[serde(default = "default_sl_vol_extra_atr")]
    pub sl_vol_extra_atr: f64,
    #[serde(default = "default_wick_off_lookback")]
    pub wick_off_lookback: usize,
    #[serde(default = "default_wick_off_buffer_atr")]
    pub wick_off_buffer_atr: f64,
    #[serde(default = "default_max_sl_asia_atr")]
    pub max_sl_asia_atr: f64,
    #[serde(default = "default_max_sl_europe_atr")]
    pub max_sl_europe_atr: f64,
    #[serde(default = "default_max_sl_usa_atr")]
    pub max_sl_usa_atr: f64,
    #[serde(default = "default_min_pullback_atr")]
    pub min_pullback_atr: f64,
    #[serde(default = "default_target_clear_atr")]
    pub target_clear_atr: f64,
    #[serde(default = "default_no_chase_tp1_atr")]
    pub no_chase_tp1_atr: f64,
    #[serde(default = "default_sl_wide_threshold_atr")]
    pub sl_wide_threshold_atr: f64,
    #[serde(default)]
    pub flow: FlowConfig,
    #[serde(default)]
    pub probability: ProbabilityConfig,
}

fn default_sl_trap_extra_atr() -> f64 { 0.30 }
fn default_sl_wick_extra_atr() -> f64 { 0.20 }
fn default_sl_vol_extra_atr() -> f64 { 0.25 }
fn default_wick_off_lookback() -> usize { 6 }
fn default_wick_off_buffer_atr() -> f64 { 0.45 }
fn default_max_sl_asia_atr() -> f64 { 1.60 }
fn default_max_sl_europe_atr() -> f64 { 1.90 }
fn default_max_sl_usa_atr() -> f64 { 2.20 }
fn default_min_pullback_atr() -> f64 { 0.18 }
fn default_target_clear_atr() -> f64 { 0.08 }
fn default_no_chase_tp1_atr() -> f64 { 0.20 }
fn default_sl_wide_threshold_atr() -> f64 { 2.00 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowConfig {
    #[serde(default = "default_flow_lookback")]
    pub flow_lookback: usize,
    #[serde(default = "default_low_flow_vol_ratio")]
    pub low_flow_vol_ratio: f64,
    #[serde(default = "default_high_flow_vol_ratio")]
    pub high_flow_vol_ratio: f64,
    #[serde(default = "default_low_flow_atr_ratio")]
    pub low_flow_atr_ratio: f64,
    #[serde(default = "default_high_flow_atr_ratio")]
    pub high_flow_atr_ratio: f64,
    #[serde(default = "default_low_liq_tp1_max_atr")]
    pub low_liq_tp1_max_atr: f64,
    #[serde(default = "default_low_liq_tp2_max_atr")]
    pub low_liq_tp2_max_atr: f64,
    #[serde(default = "default_low_liq_tp3_max_atr")]
    pub low_liq_tp3_max_atr: f64,
    #[serde(default = "default_mid_liq_tp1_max_atr")]
    pub mid_liq_tp1_max_atr: f64,
    #[serde(default = "default_mid_liq_tp2_max_atr")]
    pub mid_liq_tp2_max_atr: f64,
    #[serde(default = "default_mid_liq_tp3_max_atr")]
    pub mid_liq_tp3_max_atr: f64,
    #[serde(default = "default_low_liq_min_step_atr")]
    pub low_liq_min_step_atr: f64,
    #[serde(default = "default_low_flow_prob_penalty")]
    pub low_flow_prob_penalty: f64,
    #[serde(default = "default_flow_trap_wick_ratio")]
    pub flow_trap_wick_ratio: f64,
}

impl Default for FlowConfig {
    fn default() -> Self {
        Self {
            flow_lookback: default_flow_lookback(),
            low_flow_vol_ratio: default_low_flow_vol_ratio(),
            high_flow_vol_ratio: default_high_flow_vol_ratio(),
            low_flow_atr_ratio: default_low_flow_atr_ratio(),
            high_flow_atr_ratio: default_high_flow_atr_ratio(),
            low_liq_tp1_max_atr: default_low_liq_tp1_max_atr(),
            low_liq_tp2_max_atr: default_low_liq_tp2_max_atr(),
            low_liq_tp3_max_atr: default_low_liq_tp3_max_atr(),
            mid_liq_tp1_max_atr: default_mid_liq_tp1_max_atr(),
            mid_liq_tp2_max_atr: default_mid_liq_tp2_max_atr(),
            mid_liq_tp3_max_atr: default_mid_liq_tp3_max_atr(),
            low_liq_min_step_atr: default_low_liq_min_step_atr(),
            low_flow_prob_penalty: default_low_flow_prob_penalty(),
            flow_trap_wick_ratio: default_flow_trap_wick_ratio(),
        }
    }
}

fn default_flow_lookback() -> usize { 34 }
fn default_low_flow_vol_ratio() -> f64 { 0.78 }
fn default_high_flow_vol_ratio() -> f64 { 1.35 }
fn default_low_flow_atr_ratio() -> f64 { 0.82 }
fn default_high_flow_atr_ratio() -> f64 { 1.18 }
fn default_low_liq_tp1_max_atr() -> f64 { 0.26 }
fn default_low_liq_tp2_max_atr() -> f64 { 0.43 }
fn default_low_liq_tp3_max_atr() -> f64 { 0.62 }
fn default_mid_liq_tp1_max_atr() -> f64 { 0.38 }
fn default_mid_liq_tp2_max_atr() -> f64 { 0.68 }
fn default_mid_liq_tp3_max_atr() -> f64 { 0.95 }
fn default_low_liq_min_step_atr() -> f64 { 0.14 }
fn default_low_flow_prob_penalty() -> f64 { 14.0 }
fn default_flow_trap_wick_ratio() -> f64 { 0.58 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbabilityConfig {
    #[serde(default = "default_min_tp1_prob")]
    pub min_tp1_prob: u32,
    #[serde(default = "default_min_tp2_prob")]
    pub min_tp2_prob: u32,
}

impl Default for ProbabilityConfig {
    fn default() -> Self {
        Self {
            min_tp1_prob: default_min_tp1_prob(),
            min_tp2_prob: default_min_tp2_prob(),
        }
    }
}

fn default_min_tp1_prob() -> u32 { 60 }
fn default_min_tp2_prob() -> u32 { 42 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrapGuardConfig {
    pub trap_score_threshold: f64,
    pub trap_volume_z: f64,
    pub wick_trap_atr: f64,
    pub cooldown_bars: usize,
    #[serde(default = "default_cooldown_band_low_bars")]
    pub cooldown_band_low_bars: usize,
    #[serde(default = "default_cooldown_band_mid_bars")]
    pub cooldown_band_mid_bars: usize,
    #[serde(default = "default_cooldown_band_high_bars")]
    pub cooldown_band_high_bars: usize,
    #[serde(default = "default_cooldown_band_mid_score")]
    pub cooldown_band_mid_score: f64,
    #[serde(default = "default_cooldown_band_high_score")]
    pub cooldown_band_high_score: f64,
    #[serde(default = "default_shock_freeze_bars")]
    pub shock_freeze_bars: usize,
    #[serde(default = "default_deep_reclaim_bars")]
    pub deep_reclaim_bars: usize,
    #[serde(default = "default_min_swing_distance_atr")]
    pub min_swing_distance_atr: f64,
}

fn default_cooldown_band_low_bars() -> usize {
    4
}

fn default_cooldown_band_mid_bars() -> usize {
    6
}

fn default_cooldown_band_high_bars() -> usize {
    7
}

fn default_cooldown_band_mid_score() -> f64 {
    75.0
}

fn default_cooldown_band_high_score() -> f64 {
    90.0
}

fn default_shock_freeze_bars() -> usize {
    5
}

fn default_deep_reclaim_bars() -> usize {
    3
}

fn default_min_swing_distance_atr() -> f64 {
    0.45
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub timezone: String,
    pub asia_start: String,
    pub asia_end: String,
    pub europe_start: String,
    pub europe_end: String,
    pub usa_start: String,
    pub usa_end: String,
    pub idx_start: String,
    pub idx_end: String,
    pub forex_rollover_avoid_start: String,
    pub forex_rollover_avoid_end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub request_timeout_secs: u64,
    pub auth_token_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertsConfig {
    pub enabled: bool,
    pub poll_interval_secs: u64,
    pub batch_size: usize,
    pub dedupe_ttl_secs: u64,
    pub telegram: TelegramAlertConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramAlertConfig {
    pub enabled: bool,
    pub bot_token_env: String,
    pub chat_id_env: String,
    pub api_base_url: String,
    pub parse_mode: String,
    pub disable_web_page_preview: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub enabled: bool,
    pub url_env: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout_secs: u64,
    pub migrate_on_start: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub url_env: String,
    pub key_prefix: String,
    pub pool_size: usize,
    pub ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub scan_interval_secs: u64,
    pub shutdown_timeout_secs: u64,
    pub max_symbol_tasks: usize,
    pub health_stale_after_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourcesConfig {
    pub primary: String,
    pub fallback: String,
    pub candle_limit: usize,
    pub scanning_mode: String,
    pub retry: RetryConfig,
    pub tradingview: TradingViewDataSourceConfig,
    pub twelvedata: TwelveDataDataSourceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingViewDataSourceConfig {
    pub enabled: bool,
    pub auth_token_env: String,
    pub session_id_env: String,
    pub session_signature_env: String,
    pub device_token_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwelveDataDataSourceConfig {
    pub enabled: bool,
    pub base_url: String,
    pub api_key_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeConfig {
    pub platform: String,
    pub testnet: bool,
    pub rate_limit_per_second: usize,
    pub binance: BinanceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceConfig {
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub api_key_env: String,
    pub api_secret_env: String,
    pub account_type: String,
    pub recv_window: u64,
    pub testnet: bool,
    pub market_data_mode: String,
    pub http_poll_interval: u64,
    pub rest_url: String,
    pub websocket_url: String,
    pub ws: BinanceWsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinanceWsConfig {
    pub enabled: bool,
    pub url: String,
    pub max_streams_per_connection: usize,
    pub reconnect_base_delay_ms: u64,
    pub reconnect_max_delay_ms: u64,
    pub candle_buffer_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingConfig {
    pub enabled: bool,
    pub mode: String,
    pub order_type: String,
    pub use_oco: bool,
    pub trailing_stop_after_tp1: bool,
    pub trailing_stop_atr: f64,
    pub scaling: ScalingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingConfig {
    pub ew1_pct: f64,
    pub ew2_pct: f64,
    pub ew3_pct: f64,
    pub deep_add_pct: f64,
    pub tp1_close_pct: f64,
    pub tp2_close_pct: f64,
    pub tp3_close_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioConfig {
    pub total_capital_usdt: f64,
    pub reserve_pct: f64,
    pub max_open_positions: usize,
    pub max_positions_per_asset: usize,
    pub allocation: AllocationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocationConfig {
    pub btc_pct: f64,
    pub altcoin_pct: f64,
    pub gold_pct: f64,
    pub forex_pct: f64,
    pub stocks_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    pub max_risk_per_trade_pct: f64,
    pub position_sizing_method: String,
    pub min_risk_reward: f64,
    pub max_trades_per_day: usize,
    pub cooldown_after_loss_secs: u64,
    pub drawdown: DrawdownConfig,
    pub kill_switch: KillSwitchConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawdownConfig {
    pub warning_pct: f64,
    pub caution_pct: f64,
    pub critical_pct: f64,
    pub max_account_drawdown_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillSwitchConfig {
    pub enabled: bool,
    pub close_positions_on_trigger: bool,
    pub max_api_errors: usize,
    pub manual_restart_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfig {
    pub enabled: bool,
    pub start_date: String,
    pub end_date: String,
    pub initial_capital: f64,
    pub data_source: String,
    pub fees: FeeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeConfig {
    pub maker_fee_pct: f64,
    pub taker_fee_pct: f64,
    pub slippage_model: String,
    pub slippage_bps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolConfig {
    pub symbol: String,
    pub asset_class: AssetClass,
    pub exchange: String,
    pub timeframes: Vec<String>,
    /// Override the data source for this symbol: "binance" | "tradingview" | "twelvedata".
    /// When absent, derived from `exchange` (binance → "binance", else global primary).
    #[serde(default)]
    pub data_source: Option<String>,
}

/// Per-proxy-symbol config: separate TradingView and Twelve Data identifiers + preferred source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxySymbolEntry {
    /// TradingView symbol (e.g. "OANDA:XAUUSD"). Used when source = "tradingview".
    #[serde(default)]
    pub tradingview: Option<String>,
    /// Twelve Data symbol (e.g. "XAU/USD"). Used when source = "twelvedata".
    #[serde(default)]
    pub twelvedata: Option<String>,
    /// Preferred data source: "tradingview" | "twelvedata". Defaults to "twelvedata".
    #[serde(default = "default_proxy_source")]
    pub source: String,
}

fn default_proxy_source() -> String {
    "twelvedata".to_owned()
}

impl ProxySymbolEntry {
    /// Returns the symbol string for the preferred source.
    pub fn symbol(&self) -> &str {
        match self.source.as_str() {
            "tradingview" => self.tradingview.as_deref().unwrap_or(""),
            _ => self.twelvedata.as_deref().unwrap_or(""),
        }
    }

    /// Convenience constructor for tests: creates a Binance-routed proxy entry.
    /// Symbol is fetched via the default (Binance) data source in test scenarios.
    pub fn from_binance(symbol: impl Into<String>) -> Self {
        let sym = symbol.into();
        Self {
            tradingview: None,
            twelvedata: Some(sym),
            source: default_proxy_source(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxySymbols {
    pub xauusd: ProxySymbolEntry,
    pub ihsg: ProxySymbolEntry,
    pub dxy: ProxySymbolEntry,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_config() {
        let config = AppConfig::from_default_toml().expect("default config parses");
        assert_eq!(config.indicators.rsi_length, 14);
        assert_eq!(config.symbols[0].asset_class, AssetClass::Btc);
        assert_eq!(
            config.trading.scaling.ew1_pct
                + config.trading.scaling.ew2_pct
                + config.trading.scaling.ew3_pct
                + config.trading.scaling.deep_add_pct,
            100.0
        );
    }
}
