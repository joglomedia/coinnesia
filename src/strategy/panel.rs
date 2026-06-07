//! Pine V61.x reference panel data structure (sub-phase 1.7.10).
//!
//! `PanelReport` carries every row emitted by the TradingView reference
//! panels — `PUTUSAN`, `TRADE SCORE`, `BIAS`, `CONF`, `SESI`, `FLOW`,
//! `EW1/2/3`, `DEEP RISK`, `TRAP GATE`, `ENTRY IDEAL`, `WAKTU ENTRY`, `SL`,
//! `TP1/2/3` (price + probability + ETA), `RECLAIM` — so that the Telegram
//! formatter and `GET /signals/:symbol` API surfaces (1.7.11) can render the
//! full panel without recomputing anything. The struct is fully serde-driven
//! so it round-trips through Valkey and Postgres without bespoke encoding.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{AssetClass, Candle, Timeframe};

use super::{
    confidence::ConfidenceScore,
    entry_plan::{EntryPlan, PriceBand},
    guard_state::GuardState,
    plan_context::FlowState,
    session::{session_time_range_string, MarketSession},
    sl_engine::SlWidth,
    tp_engine::ProbLabel,
    trap_guard::{TrapDecision, TrapType},
    SignalDirection, SignalState,
};

/// EW row status. Pine emits `TOUCHED` / `VALID` / `WATCH` / `MAP` and a
/// dedicated `DEEP RECLAIM` label when the V61.4 deep-add reclaim counter is
/// active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EwStatus {
    Touched,
    Valid,
    Watch,
    Map,
    DeepReclaim,
}

impl EwStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Touched => "TOUCHED",
            Self::Valid => "VALID",
            Self::Watch => "WATCH",
            Self::Map => "MAP",
            Self::DeepReclaim => "DEEP RECLAIM",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeepStatus {
    Map,
    Danger,
    ReclaimOk,
    Invalid,
}

impl DeepStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Map => "MAP",
            Self::Danger => "DANGER",
            Self::ReclaimOk => "RECLAIM OK",
            Self::Invalid => "INVALID",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TradeScoreStatus {
    NoDirection,
    /// Sub-phase 1.7.14 Gap 6 — TRAP status wins over `NoDirection` when the
    /// trap guard is actively blocking emission. Pine renders this as the
    /// red `TRAP` chip next to the trade score even on `NO TRADE` bars.
    TrapBlocked,
    Weak,
    Ok,
    Strong,
}

impl TradeScoreStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::NoDirection => "NO DIRECTION",
            Self::TrapBlocked => "TRAP",
            Self::Weak => "WEAK",
            Self::Ok => "OK",
            Self::Strong => "STRONG",
        }
    }
}

/// Closed-open `[from, until)` timestamp window. Used for `NEXT TRADE`,
/// `WAKTU ENTRY`, and `ETA TP*` rows. Both bounds are absolute UTC instants
/// so the renderer can localize per-user without ambiguity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimestampRange {
    pub from: DateTime<Utc>,
    pub until: DateTime<Utc>,
}

impl TimestampRange {
    pub const fn new(from: DateTime<Utc>, until: DateTime<Utc>) -> Self {
        Self { from, until }
    }
}

/// Per-asset panel extras populated for the IDX/Gold/Forex variants of the
/// Telegram panel and the `GET /panel/:symbol` HTML page. Each block is
/// `Option<…>` so the renderer can omit the row when the underlying gauge
/// did not fire.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct PanelAssetExtras {
    /// V1 Gold XAUUSD proxy bias (Long/Short/Wait). Present when the panel
    /// belongs to a Gold-classified symbol with proxy data threaded in.
    pub xauusd_bias: Option<SignalDirection>,
    /// V58 Forex HTF aggregate (H4 + D1). `htf_long_score` / `htf_short_score`
    /// out of `htf_max_score` votes; `block_long` / `block_short` mirror the
    /// `blockCounterHTF` hard gate decision.
    pub htf_long_score: Option<u32>,
    pub htf_short_score: Option<u32>,
    pub htf_max_score: Option<u32>,
    pub htf_block_long: Option<bool>,
    pub htf_block_short: Option<bool>,
    /// V5 IDX flow gauges. Each is `None` when the indicator is still in its
    /// warm-up window. `rvol_value` is the V5 IDX RVOL reading, `cmf_value`
    /// the 20-period CMF, `obv_slope_value` the OBV slope, `rs_vs_ihsg_value`
    /// the relative-strength delta against IHSG.
    pub rvol_value: Option<f64>,
    pub cmf_value: Option<f64>,
    pub obv_slope_value: Option<f64>,
    pub rs_vs_ihsg_value: Option<f64>,
}

/// Full panel payload mirroring the Pine reference panels. Fields the strategy
/// could not produce (no entry plan, no TPs, etc.) come back as `None` so the
/// renderer can emit a `"—"` placeholder without re-deriving the missing
/// columns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelReport {
    pub asset_class: AssetClass,
    pub putusan_text: String,
    pub trade_score: u32,
    pub trade_score_status: TradeScoreStatus,
    pub next_trade_window: Option<TimestampRange>,
    pub next_trade_expires_at: Option<DateTime<Utc>>,
    pub bias_text: String,
    pub conf_text: String,
    pub session_text: String,
    pub flow_text: String,
    pub flow_state: FlowState,
    pub ew1_status: EwStatus,
    pub ew2_status: EwStatus,
    pub ew3_status: EwStatus,
    pub deep_status: DeepStatus,
    pub trap_gate_text: String,
    pub entry_ideal: Option<f64>,
    pub waktu_entry: Option<TimestampRange>,
    pub waktu_entry_expires_at: Option<DateTime<Utc>>,
    pub sl_wide_label: Option<SlWidth>,
    pub sl_atr_distance: Option<f64>,
    pub tp1_prob: Option<u32>,
    pub tp2_prob: Option<u32>,
    pub tp3_prob: Option<u32>,
    pub tp1_label: Option<ProbLabel>,
    pub tp2_label: Option<ProbLabel>,
    pub tp3_label: Option<ProbLabel>,
    pub eta_tp1: Option<TimestampRange>,
    pub eta_tp2: Option<TimestampRange>,
    pub eta_tp3: Option<TimestampRange>,
    pub reclaim_price: Option<f64>,
    // Sub-phase 1.7.15 — Pine reference panel renders each EW / DEEP RISK / SL
    // / TP row as `{price} | {status}`. The status halves were already
    // available; these fields surface the price halves so the JSON export,
    // Telegram formatter, and `/panel/:symbol` page can render the full Pine
    // row even on Wait bars where the projection comes from `map_plan`.
    // Each is `None` when no entry plan (real or MAP) was built.
    #[serde(default)]
    pub ew1_price: Option<f64>,
    #[serde(default)]
    pub ew2_price: Option<f64>,
    #[serde(default)]
    pub ew3_price: Option<f64>,
    #[serde(default)]
    pub deep_price: Option<f64>,
    #[serde(default)]
    pub sl_price: Option<f64>,
    #[serde(default)]
    pub tp1_price: Option<f64>,
    #[serde(default)]
    pub tp2_price: Option<f64>,
    #[serde(default)]
    pub tp3_price: Option<f64>,
    #[serde(default)]
    pub extras: PanelAssetExtras,
}

/// Inputs required to build a `PanelReport`. Borrowed so the strategy module
/// can populate the panel at any short-circuit point without cloning candles
/// or guard state.
pub struct PanelInputs<'a> {
    pub asset_class: AssetClass,
    pub state: SignalState,
    pub direction: SignalDirection,
    pub confidence: ConfidenceScore,
    pub min_confidence: f64,
    pub session: MarketSession,
    pub timeframe: Timeframe,
    pub latest: &'a Candle,
    pub atr: f64,
    pub adx: f64,
    pub sideways: bool,
    pub shock_active: bool,
    pub flow_state: FlowState,
    pub flow_trap_block: bool,
    pub trap: &'a TrapDecision,
    pub guard: &'a GuardState,
    pub plan: Option<&'a EntryPlan>,
    pub reason: &'a str,
    /// Asset-class-specific extras populated from the active IndicatorSnapshot.
    pub extras: PanelAssetExtras,
}

impl PanelReport {
    /// Build the panel from a fully-populated [`PanelInputs`]. Missing entry
    /// plan fields collapse to `None` so the renderer can show placeholders
    /// without crashing on partial data.
    pub fn build(inputs: &PanelInputs<'_>) -> Self {
        let trade_score = compute_trade_score(inputs);
        let trade_score_status =
            trade_score_status(trade_score, inputs.state, inputs.trap.blocks_signal);
        let putusan_text = putusan_text(inputs.state).to_owned();
        let bias_text = bias_text(inputs.confidence);
        let conf_text = conf_text(
            inputs.confidence,
            inputs.min_confidence,
        );
        let session_text = session_text(inputs.session, inputs.asset_class, &inputs.extras);
        let flow_text = flow_text(inputs.flow_state, inputs.flow_trap_block).to_owned();
        let trap_gate_text = trap_gate_text(inputs).to_owned();

        let (
            ew1_status,
            ew2_status,
            ew3_status,
            deep_status,
            entry_ideal,
            reclaim_price,
            ew1_price,
            ew2_price,
            ew3_price,
            deep_price,
        ) = match inputs.plan {
            Some(plan) => {
                let ew1 = ew_status(
                    inputs.state,
                    inputs.direction,
                    inputs.guard,
                    inputs.latest.close,
                    inputs.atr,
                    &plan.ew1,
                );
                let ew2 = ew_status(
                    inputs.state,
                    inputs.direction,
                    inputs.guard,
                    inputs.latest.close,
                    inputs.atr,
                    &plan.ew2,
                );
                let ew3 = ew_status(
                    inputs.state,
                    inputs.direction,
                    inputs.guard,
                    inputs.latest.close,
                    inputs.atr,
                    &plan.ew3,
                );
                let deep = deep_status(inputs.state, inputs.guard, plan);
                let ew1_mid = midpoint(&plan.ew1);
                let ew2_mid = midpoint(&plan.ew2);
                let ew3_mid = midpoint(&plan.ew3);
                let deep_mid = midpoint(&plan.deep_add);
                // Sub-phase 1.7.16 Gap B — Pine `reclaim` (Gold V1 line 1499):
                //   activeLong  → ew1 - (ew1 - ew3) * 0.35
                //   activeShort → ew1 + (ew3 - ew1) * 0.35
                // Both collapse to `ew1 + (ew3 - ew1) * 0.35` because LONG has
                // ew1 > ew3 (entries below price) and SHORT has ew1 < ew3
                // (entries above price), so the signed delta carries the
                // correct direction without an explicit branch. The previous
                // `midpoint(deep_add)` source duplicated `deep_price` and bore
                // no relation to the Pine reclaim threshold (the price the
                // structure must reclaim to re-arm the trade).
                let reclaim = ew1_mid + 0.35 * (ew3_mid - ew1_mid);
                (
                    ew1,
                    ew2,
                    ew3,
                    deep,
                    Some(plan.entry_ideal),
                    Some(reclaim),
                    Some(ew1_mid),
                    Some(ew2_mid),
                    Some(ew3_mid),
                    Some(deep_mid),
                )
            }
            None => (
                EwStatus::Map,
                EwStatus::Map,
                EwStatus::Map,
                if inputs.guard.shock_freeze_bars > 0 {
                    DeepStatus::Invalid
                } else {
                    DeepStatus::Map
                },
                None,
                None,
                None,
                None,
                None,
                None,
            ),
        };

        let waktu_entry = entry_window(inputs);
        let waktu_entry_expires_at = waktu_entry.map(|w| w.until);

        let next_trade_window = next_trade_window(inputs);
        let next_trade_expires_at = next_trade_window.map(|w| w.until);

        let (sl_wide_label, sl_atr_distance, sl_price) = inputs
            .plan
            .map(|p| {
                (
                    Some(p.stop_loss.width),
                    Some(p.stop_loss.atr_distance),
                    Some(p.stop_loss.price),
                )
            })
            .unwrap_or((None, None, None));

        let (
            tp1_prob,
            tp2_prob,
            tp3_prob,
            tp1_label,
            tp2_label,
            tp3_label,
            tp1_price,
            tp2_price,
            tp3_price,
        ) = inputs
            .plan
            .map(|p| {
                (
                    Some(p.take_profits.tp1_prob),
                    Some(p.take_profits.tp2_prob),
                    Some(p.take_profits.tp3_prob),
                    Some(p.take_profits.tp1_label),
                    Some(p.take_profits.tp2_label),
                    Some(p.take_profits.tp3_label),
                    Some(p.take_profits.tp1),
                    Some(p.take_profits.tp2),
                    Some(p.take_profits.tp3_optional),
                )
            })
            .unwrap_or((None, None, None, None, None, None, None, None, None));

        let (eta_tp1, eta_tp2, eta_tp3) = match inputs.plan {
            Some(plan) => (
                Some(eta_window(
                    inputs.timeframe,
                    inputs.latest.ts,
                    inputs.latest.close,
                    plan.take_profits.tp1,
                    inputs.atr,
                    inputs.session,
                )),
                Some(eta_window(
                    inputs.timeframe,
                    inputs.latest.ts,
                    inputs.latest.close,
                    plan.take_profits.tp2,
                    inputs.atr,
                    inputs.session,
                )),
                Some(eta_window(
                    inputs.timeframe,
                    inputs.latest.ts,
                    inputs.latest.close,
                    plan.take_profits.tp3_optional,
                    inputs.atr,
                    inputs.session,
                )),
            ),
            None => (None, None, None),
        };

        Self {
            asset_class: inputs.asset_class,
            putusan_text,
            trade_score,
            trade_score_status,
            next_trade_window,
            next_trade_expires_at,
            bias_text,
            conf_text,
            session_text,
            flow_text,
            flow_state: inputs.flow_state,
            ew1_status,
            ew2_status,
            ew3_status,
            deep_status,
            trap_gate_text,
            entry_ideal,
            waktu_entry,
            waktu_entry_expires_at,
            sl_wide_label,
            sl_atr_distance,
            tp1_prob,
            tp2_prob,
            tp3_prob,
            tp1_label,
            tp2_label,
            tp3_label,
            eta_tp1,
            eta_tp2,
            eta_tp3,
            reclaim_price,
            ew1_price,
            ew2_price,
            ew3_price,
            deep_price,
            sl_price,
            tp1_price,
            tp2_price,
            tp3_price,
            extras: inputs.extras,
        }
    }
}

fn putusan_text(state: SignalState) -> &'static str {
    match state {
        SignalState::Long => "LONG",
        SignalState::Short => "SHORT",
        SignalState::Wait => "NO TRADE",
        SignalState::Freeze => "FREEZE",
    }
}

/// Pine `tradeScore` composite. Starts from the chosen-side confidence, then
/// applies ATR-distance, trap, ADX, sideways, and shock adjustments. Clamped
/// to `[0, 100]`.
fn compute_trade_score(inputs: &PanelInputs<'_>) -> u32 {
    let conf = inputs.confidence.score(inputs.direction);
    // dist_atr penalty: fall off when current price is far from the ideal
    // entry. Without an entry plan we treat dist as zero (no penalty).
    let dist_atr = inputs
        .plan
        .map(|p| {
            if inputs.atr <= 0.0 {
                0.0
            } else {
                (inputs.latest.close - p.entry_ideal).abs() / inputs.atr
            }
        })
        .unwrap_or(0.0);
    let dist_penalty = (dist_atr - 0.35).max(0.0) * 18.0;
    let trap_penalty = inputs.trap.penalty * 0.25;
    let adx_bonus = if inputs.adx >= 20.0 { 8.0 } else { -5.0 };
    let sideways_pen = if inputs.sideways { 8.0 } else { 0.0 };
    let shock_pen = if inputs.shock_active { 30.0 } else { 0.0 };
    let raw = conf - dist_penalty - trap_penalty + adx_bonus - sideways_pen - shock_pen;
    raw.clamp(0.0, 100.0).round() as u32
}

fn trade_score_status(
    score: u32,
    state: SignalState,
    trap_blocks_signal: bool,
) -> TradeScoreStatus {
    // Sub-phase 1.7.14 Gap 6 — TRAP status takes precedence on Wait/Freeze
    // bars where the trap guard is the reason emission is blocked. Pine
    // surfaces this even when PUTUSAN = NO TRADE so the user sees *why*.
    if trap_blocks_signal {
        return TradeScoreStatus::TrapBlocked;
    }
    if matches!(state, SignalState::Wait | SignalState::Freeze) {
        return TradeScoreStatus::NoDirection;
    }
    if score >= 70 {
        TradeScoreStatus::Strong
    } else if score >= 50 {
        TradeScoreStatus::Ok
    } else {
        TradeScoreStatus::Weak
    }
}

/// Pine `BIAS` row. Always renders `Bias : MAP {LONG|SHORT} {N}%` where the
/// side is the one with the higher weighted analog score (Pine ties go to
/// LONG, matching `longConf >= shortConf`). Sub-phase 1.7.16 Gaps A+E —
/// Pine's `biasFullText` (TV_GOLD_PAXG_XAUT_V1.pine.txt line 1138) prefixes
/// every BIAS row with `"Bias : "` and always uses `MAP` regardless of
/// emission state, so we mirror that even on `Long`/`Short` bars. The
/// percentage is the weighted `ConfidenceScore` (Pine `longConf`/`shortConf`,
/// `int(round(clamp(longScore, 0, 100)))`) — *not* the layer vote ratio
/// that sub-phase 1.7.14 Gap 2 incorrectly introduced.
fn bias_text(confidence: ConfidenceScore) -> String {
    let (label, pct) = dominant_side(confidence);
    format!("Bias : MAP {} {}%", label, pct)
}

/// Pine `CONF` row: `{BELI|JUAL} {N}% | MIN {min}%` where the side is the
/// dominant weighted-confidence side (Pine line 1495 `confVal`). Pine never
/// prints both sides on `Wait` — it mirrors the stronger side and shows the
/// `MIN` threshold gate. Sub-phase 1.7.16 Gap A — restored to the weighted
/// analog `ConfidenceScore` after sub-phase 1.7.14 Gap 2's vote-ratio
/// regression.
fn conf_text(confidence: ConfidenceScore, min: f64) -> String {
    let (label, pct) = dominant_side(confidence);
    let action = if label == "LONG" { "BELI" } else { "JUAL" };
    format!("{} {}% | MIN {:.0}%", action, pct, min)
}

/// Pine `strongestBiasIsLong = longConf >= shortConf` — ties go to LONG so
/// the panel never blanks out on a neutral bar. Returns `("LONG", longConf)`
/// or `("SHORT", shortConf)`, both rounded to integer percentages as Pine
/// renders via `str.tostring(longConf)`.
fn dominant_side(confidence: ConfidenceScore) -> (&'static str, u32) {
    let long_pct = confidence.score(SignalDirection::Long).round() as u32;
    let short_pct = confidence.score(SignalDirection::Short).round() as u32;
    if long_pct >= short_pct {
        ("LONG", long_pct)
    } else {
        ("SHORT", short_pct)
    }
}

fn session_text(
    session: MarketSession,
    asset_class: AssetClass,
    extras: &PanelAssetExtras,
) -> String {
    let label = match session {
        MarketSession::Asia => "ASIA",
        MarketSession::Europe => "EROPA",
        MarketSession::Usa => "USA",
        MarketSession::Idx => "IDX",
        MarketSession::RolloverAvoid => "ROLLOVER",
    };
    let mut text = format!("{} | {}", label, session_time_range_string(session));

    // Sub-phase 1.7.14 Gap 3 — per-asset SESI annotations. Pine's Gold V1
    // panel appends `THIN EXCHANGE` whenever the active session lacks COMEX
    // floor depth (everything outside the USA overlap) and a `XAU JUAL` /
    // `XAU BELI` chip driven by the XAUUSD proxy bias. Other asset classes
    // leave the SESI row at its base label.
    if asset_class == AssetClass::Gold {
        if matches!(
            session,
            MarketSession::Asia | MarketSession::Europe | MarketSession::RolloverAvoid
        ) {
            text.push_str(" | THIN EXCHANGE");
        }
        match extras.xauusd_bias {
            Some(SignalDirection::Long) => text.push_str(" | XAU BELI"),
            Some(SignalDirection::Short) => text.push_str(" | XAU JUAL"),
            _ => {}
        }
    }
    text
}

fn flow_text(flow: FlowState, flow_trap_block: bool) -> &'static str {
    // Sub-phase 1.7.14 Gap 5 — strings ported byte-for-byte from the Pine
    // reference panel so the FLOW row matches captured TradingView snapshots.
    if flow_trap_block {
        return "Waspada stop hunt";
    }
    match flow {
        FlowState::Low => "Sepi \u{2192} fakeout risiko tinggi",
        FlowState::Mid => "Normal/sedang, TP harus realistis",
        FlowState::High => "Aliran kuat, TP boleh agresif",
    }
}

fn trap_gate_text(inputs: &PanelInputs<'_>) -> &'static str {
    if inputs.guard.shock_freeze_bars > 0 {
        return match inputs.guard.shock_freeze_bars {
            1 => "SHOCK FREEZE 1 BAR",
            2 => "SHOCK FREEZE 2 BAR",
            3 => "SHOCK FREEZE 3 BAR",
            4 => "SHOCK FREEZE 4 BAR",
            5 => "SHOCK FREEZE 5 BAR",
            _ => "SHOCK FREEZE",
        };
    }
    if inputs.guard.trap_cooldown_bars > 0 {
        return "TRAP COOLDOWN";
    }
    if inputs.trap.blocks_signal {
        return match inputs.direction {
            SignalDirection::Long => "LONG BLOCK",
            SignalDirection::Short => "SHORT BLOCK",
            SignalDirection::Wait => "TRAP BLOCK",
        };
    }
    match inputs.trap.trap_type {
        TrapType::Clean => "BERSIH",
        TrapType::BullTrap => "BULL TRAP",
        TrapType::BearTrap => "BEAR TRAP",
        TrapType::LiqSweep => "LIQ SWEEP",
        TrapType::Distribution => "DISTRIBUSI",
        TrapType::Accumulation => "AKUMULASI",
        TrapType::SlowHunt => "SLOW HUNT",
        TrapType::StopHunt => "STOP HUNT",
    }
}

fn ew_status(
    state: SignalState,
    direction: SignalDirection,
    guard: &GuardState,
    close: f64,
    atr: f64,
    band: &PriceBand,
) -> EwStatus {
    if guard.deep_reclaim_bars > 0 {
        return EwStatus::DeepReclaim;
    }
    if matches!(state, SignalState::Freeze) {
        return EwStatus::Map;
    }
    let touch = close >= band.low && close <= band.high;
    if touch {
        return EwStatus::Touched;
    }
    let near = atr > 0.0 && {
        let mid = (band.low + band.high) * 0.5;
        ((close - mid).abs() / atr) < 0.20
    };
    match (state, direction, near) {
        (SignalState::Long | SignalState::Short, _, _) => {
            if near {
                EwStatus::Valid
            } else {
                EwStatus::Watch
            }
        }
        (SignalState::Wait, SignalDirection::Wait, _) => EwStatus::Map,
        (SignalState::Wait, _, true) => EwStatus::Watch,
        (SignalState::Wait, _, false) => EwStatus::Map,
        (SignalState::Freeze, _, _) => EwStatus::Map,
    }
}

fn deep_status(state: SignalState, guard: &GuardState, plan: &EntryPlan) -> DeepStatus {
    if guard.shock_freeze_bars > 0 {
        return DeepStatus::Invalid;
    }
    if guard.deep_reclaim_bars > 0 {
        return DeepStatus::ReclaimOk;
    }
    if matches!(state, SignalState::Wait | SignalState::Freeze) {
        return DeepStatus::Map;
    }
    if plan.stop_loss.rejected_too_wide {
        return DeepStatus::Danger;
    }
    DeepStatus::Map
}

fn midpoint(band: &PriceBand) -> f64 {
    (band.low + band.high) * 0.5
}

fn entry_window(inputs: &PanelInputs<'_>) -> Option<TimestampRange> {
    let plan = inputs.plan?;
    if matches!(inputs.state, SignalState::Freeze) {
        return None;
    }
    let _ = plan; // band geometry currently unused in the time projection
    // Sub-phase 1.7.14 Gap 4 — Pine WAKTU ENTRY is the next single bar (same
    // shape as NEXT TRADE). The previous 2-bar `from = latest.ts` window
    // started a bar early and ran two bars long.
    let span = timeframe_minutes(inputs.timeframe).max(1);
    let from = inputs.latest.ts + Duration::minutes(span as i64);
    let until = from + Duration::minutes(span as i64);
    Some(TimestampRange::new(from, until))
}

fn next_trade_window(inputs: &PanelInputs<'_>) -> Option<TimestampRange> {
    let span = timeframe_minutes(inputs.timeframe).max(1);
    match inputs.state {
        SignalState::Long | SignalState::Short => None,
        SignalState::Freeze => {
            let bars = inputs.guard.shock_freeze_bars.max(1) as i64;
            let from = inputs.latest.ts + Duration::minutes(span as i64 * bars);
            let until = from + Duration::minutes(span as i64 * 4);
            Some(TimestampRange::new(from, until))
        }
        SignalState::Wait => {
            // Sub-phase 1.7.14 Gap 4 — Pine renders NEXT TRADE as the very next
            // candle window. The previous 3-bar span widened the WAKTU/NEXT
            // entry window by 4×, drifting the right bound several days past
            // Pine's reference. One bar matches the screenshot tooling.
            let from = inputs.latest.ts + Duration::minutes(span as i64);
            let until = from + Duration::minutes(span as i64);
            Some(TimestampRange::new(from, until))
        }
    }
}

fn eta_window(
    timeframe: Timeframe,
    ts: DateTime<Utc>,
    close: f64,
    target: f64,
    atr: f64,
    session: MarketSession,
) -> TimestampRange {
    let span = timeframe_minutes(timeframe).max(1) as i64;
    let bars_per_session = bars_per_session(session, timeframe).max(1.0);
    let move_per_bar = if atr > 0.0 {
        (atr / bars_per_session).max(atr * 0.05)
    } else {
        f64::EPSILON
    };
    let distance = (target - close).abs();
    let bars = (distance / move_per_bar).max(1.0).min(720.0); // cap 720 bars
    let mid_minutes = (bars * span as f64).round() as i64;
    let lower = (mid_minutes as f64 * 0.7).round() as i64;
    let upper = (mid_minutes as f64 * 1.3).round() as i64;
    TimestampRange::new(
        ts + Duration::minutes(lower),
        ts + Duration::minutes(upper),
    )
}

fn timeframe_minutes(tf: Timeframe) -> u32 {
    match tf {
        Timeframe::M1 => 1,
        Timeframe::M5 => 5,
        Timeframe::M15 => 15,
        Timeframe::H1 => 60,
        Timeframe::H4 => 240,
        Timeframe::D1 => 1440,
        Timeframe::W1 => 10_080,
        Timeframe::Mn1 => 43_200,
    }
}

/// Approximate "bars per active session" used to convert ATR into a per-bar
/// expected move. Pine's panel uses a similar heuristic to project ETA TPs.
fn bars_per_session(session: MarketSession, tf: Timeframe) -> f64 {
    let session_minutes: f64 = match session {
        MarketSession::Asia => 8.0 * 60.0,
        MarketSession::Europe => 5.5 * 60.0,
        MarketSession::Usa => 6.5 * 60.0,
        MarketSession::Idx => 6.0 * 60.0,
        MarketSession::RolloverAvoid => 1.25 * 60.0,
    };
    (session_minutes / timeframe_minutes(tf) as f64).max(1.0)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::indicators::regime::MarketRegime;
    use crate::strategy::trap_guard::TrapDecision;

    use super::*;

    fn dummy_candle() -> Candle {
        Candle {
            ts: Utc.with_ymd_and_hms(2026, 5, 20, 14, 0, 0).unwrap(),
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.5,
            volume: 1_000.0,
        }
    }

    fn base_inputs<'a>(
        latest: &'a Candle,
        guard: &'a GuardState,
        trap: &'a TrapDecision,
        plan: Option<&'a EntryPlan>,
    ) -> PanelInputs<'a> {
        PanelInputs {
            asset_class: AssetClass::Btc,
            state: SignalState::Wait,
            direction: SignalDirection::Wait,
            confidence: ConfidenceScore::neutral(),
            min_confidence: 60.0,
            session: MarketSession::Usa,
            timeframe: Timeframe::H1,
            latest,
            atr: 1.0,
            adx: 18.0,
            sideways: true,
            shock_active: false,
            flow_state: FlowState::Mid,
            flow_trap_block: false,
            trap,
            guard,
            plan,
            reason: "test",
            extras: PanelAssetExtras::default(),
        }
    }

    #[test]
    fn wait_panel_populates_session_and_conf_rows() {
        let candle = dummy_candle();
        let guard = GuardState::default();
        let trap = TrapDecision::allow();
        let inputs = base_inputs(&candle, &guard, &trap, None);
        let panel = PanelReport::build(&inputs);
        assert_eq!(panel.putusan_text, "NO TRADE");
        assert!(panel.session_text.contains("USA"));
        assert!(panel.session_text.contains("WIB"));
        assert!(panel.conf_text.contains("BELI"));
        assert!(panel.entry_ideal.is_none());
        assert!(panel.tp1_prob.is_none());
        assert_eq!(panel.trade_score_status, TradeScoreStatus::NoDirection);
    }

    #[test]
    fn freeze_panel_uses_shock_freeze_label() {
        let candle = dummy_candle();
        let guard = GuardState {
            shock_freeze_bars: 3,
            ..GuardState::default()
        };
        let trap = TrapDecision::allow();
        let mut inputs = base_inputs(&candle, &guard, &trap, None);
        inputs.state = SignalState::Freeze;
        inputs.shock_active = true;
        let panel = PanelReport::build(&inputs);
        assert_eq!(panel.putusan_text, "FREEZE");
        assert_eq!(panel.trap_gate_text, "SHOCK FREEZE 3 BAR");
        assert_eq!(panel.deep_status, DeepStatus::Invalid);
        assert!(panel.next_trade_window.is_some());
    }

    #[test]
    fn trade_score_clamps_into_status_buckets() {
        let candle = dummy_candle();
        let guard = GuardState::default();
        let trap = TrapDecision::allow();
        let mut inputs = base_inputs(&candle, &guard, &trap, None);
        // Strong long: high confidence, high ADX, no penalties.
        inputs.state = SignalState::Long;
        inputs.direction = SignalDirection::Long;
        inputs.confidence = ConfidenceScore::from_sides(82.0, 18.0);
        inputs.adx = 28.0;
        inputs.sideways = false;
        let panel = PanelReport::build(&inputs);
        assert!(panel.trade_score >= 70, "panel trade_score = {}", panel.trade_score);
        assert_eq!(panel.trade_score_status, TradeScoreStatus::Strong);
        // Sub-phase 1.7.16 Gap E — Pine's `biasFullText` always renders
        // `Bias : MAP {LONG|SHORT} {N}%`, even on emission bars.
        assert!(
            panel.bias_text.starts_with("Bias : MAP LONG"),
            "bias_text = {}",
            panel.bias_text
        );
    }

    #[test]
    fn timeframe_minutes_match_pine_intervals() {
        assert_eq!(timeframe_minutes(Timeframe::M5), 5);
        assert_eq!(timeframe_minutes(Timeframe::H4), 240);
        assert_eq!(timeframe_minutes(Timeframe::D1), 1440);
    }

    #[test]
    fn eta_window_orders_from_before_until() {
        let ts = Utc.with_ymd_and_hms(2026, 5, 20, 14, 0, 0).unwrap();
        let win = eta_window(
            Timeframe::H1,
            ts,
            100.0,
            104.0,
            1.0,
            MarketSession::Usa,
        );
        assert!(win.from < win.until);
        assert!(win.from > ts);
    }

    fn _regime_referenced() {
        // Keeps a path-import on MarketRegime so future tests can reuse it.
        let _ = MarketRegime::Sideways;
    }
}
