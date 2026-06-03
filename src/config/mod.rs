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
    /// V61.x per-asset overrides (sub-phase 1.7.12). Lets each evaluator branch
    /// pull knobs from TOML instead of hard-coding constants.
    #[serde(default)]
    pub assets: AssetsConfig,
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
    #[serde(default = "default_sr_near_atr")]
    pub sr_near_atr: f64,
    #[serde(default = "default_sr_daily_lookback")]
    pub sr_daily_lookback: usize,
    #[serde(default = "default_sr_weekly_lookback")]
    pub sr_weekly_lookback: usize,
    #[serde(default = "default_cmf_length")]
    pub cmf_length: usize,
    #[serde(default = "default_obv_slope_length")]
    pub obv_slope_length: usize,
    #[serde(default = "default_rvol_length")]
    pub rvol_length: usize,
    #[serde(default = "default_rvol_min")]
    pub rvol_min: f64,
    #[serde(default = "default_rs_length")]
    pub rs_length: usize,
    #[serde(default = "default_htf_ema_fast")]
    pub htf_ema_fast: usize,
    #[serde(default = "default_htf_ema_mid")]
    pub htf_ema_mid: usize,
    #[serde(default = "default_htf_ema_trend")]
    pub htf_ema_trend: usize,
    /// V61.x session volume baseline window (bars). Drives the rolling MA
    /// behind shock-z and breakout ratio gating.
    #[serde(default = "default_session_volume_baseline_length")]
    pub session_volume_baseline_length: usize,
    /// V61.x volume shock z-score threshold for the session baseline above.
    #[serde(default = "default_session_volume_shock_z")]
    pub session_volume_shock_z: f64,
    /// V61.x breakout volume ratio (current / session baseline) gate.
    #[serde(default = "default_session_breakout_volume_ratio")]
    pub session_breakout_volume_ratio: f64,
    /// V61.x break-of-structure close confirmation buffer in ATR units.
    #[serde(default = "default_bos_close_buffer_atr")]
    pub bos_close_buffer_atr: f64,
    /// V61.x change-of-character close confirmation buffer in ATR units.
    #[serde(default = "default_choch_close_buffer_atr")]
    pub choch_close_buffer_atr: f64,
    /// V61.x "equal highs/lows" tolerance band (ATR units) for liquidity pools.
    #[serde(default = "default_liquidity_equal_atr")]
    pub liquidity_equal_atr: f64,
    /// V61.x order-block validation requires volume ≥ this × volume_ma.
    #[serde(default = "default_ob_validation_vol_ratio")]
    pub ob_validation_vol_ratio: f64,
    /// V61.x momentum decay window (bars) — after this many bars without follow-through
    /// the momentum component decays toward zero.
    #[serde(default = "default_momentum_decay_bars")]
    pub momentum_decay_bars: usize,
}

fn default_ob_displacement_atr() -> f64 {
    0.65
}

fn default_sr_cluster_atr() -> f64 {
    0.35
}

fn default_sr_near_atr() -> f64 { 0.50 }
fn default_sr_daily_lookback() -> usize { 24 }
fn default_sr_weekly_lookback() -> usize { 168 }
fn default_cmf_length() -> usize { 20 }
fn default_obv_slope_length() -> usize { 10 }
fn default_rvol_length() -> usize { 20 }
fn default_rvol_min() -> f64 { 1.20 }
fn default_rs_length() -> usize { 10 }
fn default_htf_ema_fast() -> usize { 21 }
fn default_htf_ema_mid() -> usize { 55 }
fn default_htf_ema_trend() -> usize { 200 }
fn default_session_volume_baseline_length() -> usize { 34 }
fn default_session_volume_shock_z() -> f64 { 2.20 }
fn default_session_breakout_volume_ratio() -> f64 { 1.15 }
fn default_bos_close_buffer_atr() -> f64 { 0.10 }
fn default_choch_close_buffer_atr() -> f64 { 0.12 }
fn default_liquidity_equal_atr() -> f64 { 0.20 }
fn default_ob_validation_vol_ratio() -> f64 { 1.10 }
fn default_momentum_decay_bars() -> usize { 8 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub min_directional_gap: f64,
    pub min_confidence_15m: f64,
    pub min_confidence_1h: f64,
    pub min_confidence_4h: f64,
    pub min_confidence_1d: f64,
    pub structure_lookback: usize,
    pub min_structure_score: f64,
    /// V61.x minimum structural edge (long_votes − short_votes) required before the
    /// strategy will emit a directional signal. Expressed as a normalized fraction
    /// of the overall structure score weight.
    #[serde(default = "default_min_structure_edge")]
    pub min_structure_edge: f64,
    #[serde(default)]
    pub mtf: MtfConfig,
}

fn default_min_structure_edge() -> f64 {
    0.15
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
    /// V61.x entry-window micro-buffer 1 (ATR units) — tightest band around ideal entry.
    #[serde(default = "default_ew_micro_1_atr")]
    pub ew_micro_1_atr: f64,
    /// V61.x entry-window micro-buffer 2 (ATR units) — mid band.
    #[serde(default = "default_ew_micro_2_atr")]
    pub ew_micro_2_atr: f64,
    /// V61.x entry-window micro-buffer 3 (ATR units) — widest acceptable chase.
    #[serde(default = "default_ew_micro_3_atr")]
    pub ew_micro_3_atr: f64,
    /// V61.x extra buffer (ATR units) added to entry windows that open right at a
    /// session boundary, to avoid the first-bar wick noise.
    #[serde(default = "default_ew_session_open_buffer_atr")]
    pub ew_session_open_buffer_atr: f64,
    /// V61.x minimum risk-reward (TP1 / SL) before a trade can be emitted.
    #[serde(default = "default_min_rr_trade")]
    pub min_rr_trade: f64,
    #[serde(default)]
    pub flow: FlowConfig,
    #[serde(default)]
    pub probability: ProbabilityConfig,
}

fn default_ew_micro_1_atr() -> f64 { 0.06 }
fn default_ew_micro_2_atr() -> f64 { 0.12 }
fn default_ew_micro_3_atr() -> f64 { 0.20 }
fn default_ew_session_open_buffer_atr() -> f64 { 0.18 }
fn default_min_rr_trade() -> f64 { 1.6 }

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
    #[serde(default = "default_pressure_cluster_bars")]
    pub pressure_cluster_bars: usize,
    #[serde(default = "default_pressure_cluster_min")]
    pub pressure_cluster_min: usize,
    #[serde(default = "default_pressure_vol_ratio")]
    pub pressure_vol_ratio: f64,
    #[serde(default = "default_shock_range_atr")]
    pub shock_range_atr: f64,
    #[serde(default = "default_shock_body_atr")]
    pub shock_body_atr: f64,
    #[serde(default = "default_distribution_reject_bars")]
    pub distribution_reject_bars: usize,
    #[serde(default = "default_sess_vol_breakout_ratio")]
    pub sess_vol_breakout_ratio: f64,
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

fn default_pressure_cluster_bars() -> usize { 8 }
fn default_pressure_cluster_min() -> usize { 3 }
fn default_pressure_vol_ratio() -> f64 { 1.15 }
fn default_shock_range_atr() -> f64 { 2.50 }
fn default_shock_body_atr() -> f64 { 1.40 }
fn default_distribution_reject_bars() -> usize { 5 }
fn default_sess_vol_breakout_ratio() -> f64 { 1.15 }

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

/// Per-asset Pine V61.x–V62 override block (sub-phase 1.7.12).
///
/// Each sub-section captures the knobs that diverge between asset classes so the
/// evaluator branches in `src/assets/` can pull tuning from TOML instead of constants.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetsConfig {
    #[serde(default)]
    pub altcoin: AltcoinAssetConfig,
    #[serde(default)]
    pub gold: GoldAssetConfig,
    #[serde(default)]
    pub forex: ForexAssetConfig,
    #[serde(default)]
    pub stocks_idx: StocksIdxAssetConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AltcoinAssetConfig {
    /// V62 EW band compression factor for thin/illiquid alts.
    #[serde(default = "default_alt_ew_vol_compress")]
    pub alt_ew_vol_compress: f64,
    /// V62 TP compression factor when the book is thin (probability penalty companion).
    #[serde(default = "default_alt_tp_thin_compress")]
    pub alt_tp_thin_compress: f64,
    /// V62 extra SL buffer (ATR) to clear wick spikes typical on alt pairs.
    #[serde(default = "default_alt_sl_wick_buffer_atr")]
    pub alt_sl_wick_buffer_atr: f64,
    /// V62 trap-guard sensitivity multiplier (>1 = stricter).
    #[serde(default = "default_alt_trap_sensitivity")]
    pub alt_trap_sensitivity: f64,
    /// V62 minimum break body size (ATR) for valid breakout candle.
    #[serde(default = "default_alt_min_break_body_atr")]
    pub alt_min_break_body_atr: f64,
    /// V62 maximum chase distance (ATR) beyond ideal entry before invalidating.
    #[serde(default = "default_alt_max_chase_atr")]
    pub alt_max_chase_atr: f64,
    /// V62 weight applied to LTF consensus when blending into composite score.
    #[serde(default = "default_alt_ltf_weight")]
    pub alt_ltf_weight: f64,
    /// V62 HTF bias relaxation factor (<1 = HTF bias counted less for alts).
    #[serde(default = "default_alt_htf_relax")]
    pub alt_htf_relax: f64,
    /// AUTO | MAJOR | MID | MEME — Pine altcoin sensitivity preset selector.
    #[serde(default = "default_alt_profile")]
    pub alt_profile: String,
}

impl Default for AltcoinAssetConfig {
    fn default() -> Self {
        Self {
            alt_ew_vol_compress: default_alt_ew_vol_compress(),
            alt_tp_thin_compress: default_alt_tp_thin_compress(),
            alt_sl_wick_buffer_atr: default_alt_sl_wick_buffer_atr(),
            alt_trap_sensitivity: default_alt_trap_sensitivity(),
            alt_min_break_body_atr: default_alt_min_break_body_atr(),
            alt_max_chase_atr: default_alt_max_chase_atr(),
            alt_ltf_weight: default_alt_ltf_weight(),
            alt_htf_relax: default_alt_htf_relax(),
            alt_profile: default_alt_profile(),
        }
    }
}

fn default_alt_ew_vol_compress() -> f64 { 0.85 }
fn default_alt_tp_thin_compress() -> f64 { 0.78 }
fn default_alt_sl_wick_buffer_atr() -> f64 { 0.35 }
fn default_alt_trap_sensitivity() -> f64 { 1.10 }
fn default_alt_min_break_body_atr() -> f64 { 0.55 }
fn default_alt_max_chase_atr() -> f64 { 0.60 }
fn default_alt_ltf_weight() -> f64 { 1.20 }
fn default_alt_htf_relax() -> f64 { 0.85 }
fn default_alt_profile() -> String { "AUTO".to_owned() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldAssetConfig {
    /// Gold V1 session bias source mode: "session" | "proxy" | "hybrid".
    #[serde(default = "default_gold_session_bias_mode")]
    pub gold_session_bias_mode: String,
    /// Extra ATR allowance during high-impact news windows.
    #[serde(default = "default_gold_news_window_atr")]
    pub gold_news_window_atr: f64,
    /// Minimum alignment (0..1) between XAU/USD proxy and instrument before counting as confirming.
    #[serde(default = "default_gold_proxy_min_alignment")]
    pub gold_proxy_min_alignment: f64,
}

impl Default for GoldAssetConfig {
    fn default() -> Self {
        Self {
            gold_session_bias_mode: default_gold_session_bias_mode(),
            gold_news_window_atr: default_gold_news_window_atr(),
            gold_proxy_min_alignment: default_gold_proxy_min_alignment(),
        }
    }
}

fn default_gold_session_bias_mode() -> String { "hybrid".to_owned() }
fn default_gold_news_window_atr() -> f64 { 1.80 }
fn default_gold_proxy_min_alignment() -> f64 { 0.65 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForexAssetConfig {
    /// Forex V58 minimum RR per session.
    #[serde(default = "default_forex_rr_asia")]
    pub forex_rr_asia: f64,
    #[serde(default = "default_forex_rr_europe")]
    pub forex_rr_europe: f64,
    #[serde(default = "default_forex_rr_usa")]
    pub forex_rr_usa: f64,
    /// Forex V58 — block emission when LTF signal opposes HTF bias.
    #[serde(default = "default_forex_block_counter_htf")]
    pub forex_block_counter_htf: bool,
}

impl Default for ForexAssetConfig {
    fn default() -> Self {
        Self {
            forex_rr_asia: default_forex_rr_asia(),
            forex_rr_europe: default_forex_rr_europe(),
            forex_rr_usa: default_forex_rr_usa(),
            forex_block_counter_htf: default_forex_block_counter_htf(),
        }
    }
}

fn default_forex_rr_asia() -> f64 { 1.40 }
fn default_forex_rr_europe() -> f64 { 1.80 }
fn default_forex_rr_usa() -> f64 { 2.00 }
fn default_forex_block_counter_htf() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StocksIdxAssetConfig {
    /// IDX V5 minimum RVOL gate before signal emission.
    #[serde(default = "default_idx_rvol_min")]
    pub idx_rvol_min: f64,
    #[serde(default = "default_idx_cmf_length")]
    pub idx_cmf_length: usize,
    #[serde(default = "default_idx_obv_slope_bars")]
    pub idx_obv_slope_bars: usize,
    /// Minimum value-traded (notional) per bar to qualify as actionable liquidity.
    #[serde(default = "default_idx_value_traded_min")]
    pub idx_value_traded_min: f64,
    /// Minimum RS vs IHSG. Set ≤ 0 to disable.
    #[serde(default = "default_idx_rs_min")]
    pub idx_rs_min: f64,
    /// Downside-risk threshold (ATR multiple) above which signal is downgraded.
    #[serde(default = "default_idx_downside_risk_threshold")]
    pub idx_downside_risk_threshold: f64,
}

impl Default for StocksIdxAssetConfig {
    fn default() -> Self {
        Self {
            idx_rvol_min: default_idx_rvol_min(),
            idx_cmf_length: default_idx_cmf_length(),
            idx_obv_slope_bars: default_idx_obv_slope_bars(),
            idx_value_traded_min: default_idx_value_traded_min(),
            idx_rs_min: default_idx_rs_min(),
            idx_downside_risk_threshold: default_idx_downside_risk_threshold(),
        }
    }
}

fn default_idx_rvol_min() -> f64 { 1.10 }
fn default_idx_cmf_length() -> usize { 20 }
fn default_idx_obv_slope_bars() -> usize { 10 }
fn default_idx_value_traded_min() -> f64 { 500_000_000.0 }
fn default_idx_rs_min() -> f64 { 0.0 }
fn default_idx_downside_risk_threshold() -> f64 { 1.35 }

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

    #[test]
    fn default_config_exposes_new_indicator_knobs() {
        let cfg = AppConfig::from_default_toml().expect("default config parses");
        assert_eq!(cfg.indicators.session_volume_baseline_length, 34);
        assert!((cfg.indicators.session_volume_shock_z - 2.20).abs() < f64::EPSILON);
        assert!((cfg.indicators.bos_close_buffer_atr - 0.10).abs() < f64::EPSILON);
        assert!((cfg.indicators.choch_close_buffer_atr - 0.12).abs() < f64::EPSILON);
        assert!((cfg.indicators.liquidity_equal_atr - 0.20).abs() < f64::EPSILON);
        assert!((cfg.indicators.ob_validation_vol_ratio - 1.10).abs() < f64::EPSILON);
        assert_eq!(cfg.indicators.momentum_decay_bars, 8);
        assert!((cfg.strategy.min_structure_edge - 0.15).abs() < f64::EPSILON);
    }

    #[test]
    fn default_config_exposes_new_entry_plan_micros() {
        let cfg = AppConfig::from_default_toml().expect("default config parses");
        assert!((cfg.entry_plan.ew_micro_1_atr - 0.06).abs() < f64::EPSILON);
        assert!((cfg.entry_plan.ew_micro_2_atr - 0.12).abs() < f64::EPSILON);
        assert!((cfg.entry_plan.ew_micro_3_atr - 0.20).abs() < f64::EPSILON);
        assert!((cfg.entry_plan.ew_session_open_buffer_atr - 0.18).abs() < f64::EPSILON);
        assert!((cfg.entry_plan.min_rr_trade - 1.6).abs() < f64::EPSILON);
    }

    #[test]
    fn default_config_exposes_per_asset_overrides() {
        let cfg = AppConfig::from_default_toml().expect("default config parses");
        assert_eq!(cfg.assets.altcoin.alt_profile, "AUTO");
        assert!((cfg.assets.altcoin.alt_ltf_weight - 1.20).abs() < f64::EPSILON);
        assert_eq!(cfg.assets.gold.gold_session_bias_mode, "hybrid");
        assert!((cfg.assets.gold.gold_proxy_min_alignment - 0.65).abs() < f64::EPSILON);
        assert!((cfg.assets.forex.forex_rr_europe - 1.80).abs() < f64::EPSILON);
        assert!(cfg.assets.forex.forex_block_counter_htf);
        assert!((cfg.assets.stocks_idx.idx_rvol_min - 1.10).abs() < f64::EPSILON);
        assert!((cfg.assets.stocks_idx.idx_value_traded_min - 500_000_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn default_config_includes_forex_symbol_example() {
        let cfg = AppConfig::from_default_toml().expect("default config parses");
        assert!(
            cfg.symbols
                .iter()
                .any(|s| s.asset_class == AssetClass::Forex && s.symbol == "EURUSD"),
            "expected EURUSD Forex example in [[symbols]]"
        );
    }

    #[test]
    fn assets_section_falls_back_to_defaults_when_missing() {
        let toml = r#"
[indicators]
ema_fast = 20
ema_slow = 50
ema_trend = 200
atr_length = 14
rsi_length = 14
volume_ma_length = 20
adx_length = 14
adx_smoothing = 14
macd_fast = 12
macd_slow = 26
macd_signal = 9

[strategy]
min_directional_gap = 10.0
min_confidence_15m = 72.0
min_confidence_1h = 67.0
min_confidence_4h = 64.0
min_confidence_1d = 58.0
structure_lookback = 18
min_structure_score = 60.0

[entry_plan]
swing_lookback = 24
ew1_min_atr = 0.12
ew1_max_atr = 0.85
ew2_atr = 0.42
ew3_atr = 0.78
deep_add_atr = 1.12
entry_zone_atr = 0.15
tp1_atr = 0.48
tp2_atr = 0.88
tp3_atr = 1.35
tp_step_min_atr = 0.22
max_tp1_atr = 0.95
max_tp2_atr = 1.45
max_tp3_atr = 2.10
min_sl_distance_atr = 0.50
max_sl_distance_atr = 3.20
sl_extra_asia_atr = 0.15
sl_extra_europe_atr = 0.22
sl_extra_usa_atr = 0.30

[trap_guard]
trap_score_threshold = 60.0
trap_volume_z = 2.0
wick_trap_atr = 0.70
cooldown_bars = 3

[session]
timezone = "Asia/Jakarta"
asia_start = "06:00"
asia_end = "14:00"
europe_start = "14:00"
europe_end = "22:00"
usa_start = "19:00"
usa_end = "03:00"
idx_start = "09:00"
idx_end = "15:00"
forex_rollover_avoid_start = "04:55"
forex_rollover_avoid_end = "06:10"

[server]
enabled = false
host = "127.0.0.1"
port = 8080
request_timeout_secs = 10
auth_token_env = "X"

[alerts]
enabled = false
poll_interval_secs = 2
batch_size = 25
dedupe_ttl_secs = 300
[alerts.telegram]
enabled = false
bot_token_env = "X"
chat_id_env = "X"
api_base_url = "https://api.telegram.org"
parse_mode = "HTML"
disable_web_page_preview = true

[database]
enabled = false
url_env = "X"
max_connections = 10
min_connections = 1
connect_timeout_secs = 5
migrate_on_start = false

[cache]
enabled = false
url_env = "X"
key_prefix = "x"
pool_size = 10
ttl_seconds = 300

[runtime]
scan_interval_secs = 60
shutdown_timeout_secs = 15
max_symbol_tasks = 128
health_stale_after_secs = 180

[data_sources]
primary = "binance"
fallback = "tradingview"
candle_limit = 250
scanning_mode = "polling"
[data_sources.retry]
max_retries = 1
base_delay_ms = 500
max_delay_ms = 10000
[data_sources.tradingview]
enabled = false
auth_token_env = "X"
session_id_env = "X"
session_signature_env = "X"
device_token_env = "X"
[data_sources.twelvedata]
enabled = false
base_url = "https://api.twelvedata.com"
api_key_env = "X"

[exchange]
platform = "paper"
testnet = true
rate_limit_per_second = 10
[exchange.binance]
api_key_env = "X"
api_secret_env = "X"
account_type = "spot"
recv_window = 5
testnet = false
market_data_mode = "http"
http_poll_interval = 1
rest_url = "https://api.binance.com"
websocket_url = "wss://stream.binance.com:9443"
[exchange.binance.ws]
enabled = false
url = "wss://stream.binance.com/stream"
max_streams_per_connection = 200
reconnect_base_delay_ms = 1000
reconnect_max_delay_ms = 30000
candle_buffer_size = 500

[trading]
enabled = false
mode = "scan_only"
order_type = "limit"
use_oco = true
trailing_stop_after_tp1 = true
trailing_stop_atr = 0.5
[trading.scaling]
ew1_pct = 40.0
ew2_pct = 30.0
ew3_pct = 20.0
deep_add_pct = 10.0
tp1_close_pct = 50.0
tp2_close_pct = 30.0
tp3_close_pct = 20.0

[portfolio]
total_capital_usdt = 10000.0
reserve_pct = 10.0
max_open_positions = 10
max_positions_per_asset = 4
[portfolio.allocation]
btc_pct = 30.0
altcoin_pct = 25.0
gold_pct = 15.0
forex_pct = 15.0
stocks_pct = 15.0

[risk]
max_risk_per_trade_pct = 2.0
position_sizing_method = "fixed_pct"
min_risk_reward = 1.5
max_trades_per_day = 20
cooldown_after_loss_secs = 300
[risk.drawdown]
warning_pct = 3.0
caution_pct = 5.0
critical_pct = 8.0
max_account_drawdown_pct = 15.0
[risk.kill_switch]
enabled = true
close_positions_on_trigger = true
max_api_errors = 10
manual_restart_required = true

[backtest]
enabled = false
start_date = "2024-01-01"
end_date = "2025-01-01"
initial_capital = 10000.0
data_source = "api"
[backtest.fees]
maker_fee_pct = 0.02
taker_fee_pct = 0.04
slippage_model = "fixed"
slippage_bps = 5.0

[[symbols]]
symbol = "BTCUSDT"
asset_class = "btc"
exchange = "binance"
timeframes = ["1d"]

[proxy_symbols.xauusd]
twelvedata = "XAU/USD"
[proxy_symbols.ihsg]
twelvedata = "IHSG"
[proxy_symbols.dxy]
twelvedata = "DXY"
"#;
        let cfg: AppConfig = toml::from_str(toml).expect("legacy-format config still parses");
        // No [assets] section in this fixture → defaults must populate.
        assert_eq!(cfg.assets.altcoin.alt_profile, "AUTO");
        assert!(cfg.assets.forex.forex_block_counter_htf);
    }
}
