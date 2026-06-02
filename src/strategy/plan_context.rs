//! Context bundle and Pine helpers driving the V61.9 EW / SL / TP engine
//! rewrite (sub-phase 1.7.5). All inputs the entry-plan, SL, and TP modules
//! need are collected here so the engines remain pure functions of their
//! snapshot — Pine semantics ported from
//! `docs/TV_Pine_Scripts/TV_BTC_V61_9_FLOW_INTERPRETATION_PANEL_FULL.pine.txt`.

use serde::{Deserialize, Serialize};

use crate::indicators::regime::MarketRegime;

use super::{session::MarketSession, SignalDirection};

/// V61.8 liquidity / flow classification driving TP cap selection and
/// probability penalties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum FlowState {
    /// "RENDAH" — thin liquidity, narrow ATR; tightest TP caps + prob penalty.
    Low,
    /// "SEDANG" — average liquidity; medium TP caps.
    Mid,
    /// "TINGGI" — expanding volume + ATR; full TP caps allowed.
    High,
}

impl FlowState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "RENDAH",
            Self::Mid => "SEDANG",
            Self::High => "TINGGI",
        }
    }
}

/// Full input bundle for the EW/SL/TP rewrite. Anything the new engines
/// need to compute a Pine-parity plan lives here.
#[derive(Debug, Clone)]
pub struct PlanContext {
    pub direction: SignalDirection,
    pub close: f64,
    pub atr: f64,
    pub adx: f64,
    pub confidence: f64,
    /// `>= 0` — current bar trap penalty (used in `f_prob_score`).
    pub trap_score: f64,
    pub upper_wick: f64,
    pub lower_wick: f64,
    pub swing_high: f64,
    pub swing_low: f64,
    pub daily_high: f64,
    pub daily_low: f64,
    pub weekly_high: f64,
    pub weekly_low: f64,
    pub vwap: f64,
    pub ema_fast: f64,
    pub ema_slow: f64,
    pub session: MarketSession,
    pub regime: MarketRegime,
    pub flow_state: FlowState,
    pub trap_now: bool,
    pub cooldown_active: bool,
    pub vol_shock: bool,
    pub shock_active: bool,
    /// Pine `flowTrapBlock` (V61.8/V61.9) — true when thin-flow wick risk
    /// coincides with a sweep / trap / volShock event. Forces TP engine
    /// into a low-flow cap regardless of `flow_state` and applies an extra
    /// probability penalty downstream.
    pub flow_trap_block: bool,
}

impl PlanContext {
    /// Pine `regimeSidewaysPre` — anything that is not a clean trend.
    pub fn is_sideways(&self) -> bool {
        !matches!(self.regime, MarketRegime::TrendExpansion)
    }
}

// ============================================================================
// Pine helper functions (verbatim port of f_reach_*, f_cap_*, f_*_tp_*,
// f_prob_score, f_session_extra).
// ============================================================================

#[inline]
pub fn f_clamp(x: f64, lo: f64, hi: f64) -> f64 {
    lo.max(x.min(hi))
}

/// `f_reach_long(rawLevel, price, atrVal, minATR, maxATR)`
pub fn f_reach_long(raw_level: f64, price: f64, atr: f64, min_atr: f64, max_atr: f64) -> f64 {
    let dist = if atr > 0.0 {
        (price - raw_level) / atr
    } else {
        min_atr
    };
    price - atr * f_clamp(dist, min_atr, max_atr)
}

/// `f_reach_short(rawLevel, price, atrVal, minATR, maxATR)`
pub fn f_reach_short(raw_level: f64, price: f64, atr: f64, min_atr: f64, max_atr: f64) -> f64 {
    let dist = if atr > 0.0 {
        (raw_level - price) / atr
    } else {
        min_atr
    };
    price + atr * f_clamp(dist, min_atr, max_atr)
}

/// `f_cap_long(level, base, atrVal, maxATR)`
pub fn f_cap_long(level: f64, base: f64, atr: f64, max_atr: f64) -> f64 {
    level.min(base + atr * max_atr)
}

/// `f_cap_short(level, base, atrVal, maxATR)`
pub fn f_cap_short(level: f64, base: f64, atr: f64, max_atr: f64) -> f64 {
    level.max(base - atr * max_atr)
}

/// `f_long_tp_fix(tpRaw, base, prev, atrVal)`
pub fn f_long_tp_fix(tp_raw: f64, base: f64, prev: f64, atr: f64, step_min_atr: f64) -> f64 {
    tp_raw.max((base + atr * step_min_atr).max(prev + atr * step_min_atr))
}

/// `f_short_tp_fix(tpRaw, base, prev, atrVal)`
pub fn f_short_tp_fix(tp_raw: f64, base: f64, prev: f64, atr: f64, step_min_atr: f64) -> f64 {
    tp_raw.min((base - atr * step_min_atr).min(prev - atr * step_min_atr))
}

/// `f_long_tp_flow(tpRaw, base, prev, atrVal, maxDistATR, minStepATR)`
pub fn f_long_tp_flow(
    tp_raw: f64,
    base: f64,
    prev: f64,
    atr: f64,
    max_dist_atr: f64,
    min_step_atr: f64,
) -> f64 {
    let upper = tp_raw.min(base + atr * max_dist_atr);
    let lower = (base + atr * min_step_atr).max(prev + atr * min_step_atr);
    upper.max(lower)
}

/// `f_short_tp_flow(tpRaw, base, prev, atrVal, maxDistATR, minStepATR)`
pub fn f_short_tp_flow(
    tp_raw: f64,
    base: f64,
    prev: f64,
    atr: f64,
    max_dist_atr: f64,
    min_step_atr: f64,
) -> f64 {
    let lower = tp_raw.max(base - atr * max_dist_atr);
    let upper = (base - atr * min_step_atr).min(prev - atr * min_step_atr);
    lower.min(upper)
}

/// `f_prob_score(conf, distAtr, trapVal, sidewaysNow, shockNow, adxVal)`
pub fn f_prob_score(
    conf: f64,
    dist_atr: f64,
    trap_val: f64,
    sideways_now: bool,
    shock_now: bool,
    adx_val: f64,
) -> u32 {
    let raw = conf
        - (dist_atr - 0.35).max(0.0) * 18.0
        - trap_val * 0.25
        + if adx_val >= 20.0 { 8.0 } else { -5.0 }
        - if sideways_now { 8.0 } else { 0.0 }
        - if shock_now { 30.0 } else { 0.0 };
    f_clamp(raw, 0.0, 100.0).round() as u32
}

/// `f_session_extra(sess)` — extra SL pad ATR per session.
pub fn f_session_extra(
    session: MarketSession,
    sl_asia: f64,
    sl_europe: f64,
    sl_usa: f64,
) -> f64 {
    match session {
        MarketSession::Asia => sl_asia,
        MarketSession::Europe => sl_europe,
        // Idx / RolloverAvoid are Coinnesia overlays not modeled by Pine;
        // default them to the USA pad which matches Pine's else-branch.
        MarketSession::Usa
        | MarketSession::Idx
        | MarketSession::RolloverAvoid => sl_usa,
    }
}

/// Dynamic max-SL distance (ATR) per session for V61.6 sl-too-wide reject.
pub fn max_sl_distance_for(
    session: MarketSession,
    asia: f64,
    europe: f64,
    usa: f64,
) -> f64 {
    match session {
        MarketSession::Asia => asia,
        MarketSession::Europe => europe,
        _ => usa,
    }
}

/// V61.8 flow state classifier. Uses session volume ratio + ATR-vs-baseline
/// ATR ratio. Pine threshold: `lowFlowCore = volRatio <= 0.78 && atrRatio <= 0.82 && adx < 18`,
/// `highFlowCore = volRatio >= 1.35 && atrRatio >= 1.18 && rangeRatio >= 1.05 && adx >= 18`.
pub fn classify_flow(
    vol_ratio: f64,
    atr_ratio: f64,
    adx: f64,
    low_flow_vol_ratio: f64,
    low_flow_atr_ratio: f64,
    high_flow_vol_ratio: f64,
    high_flow_atr_ratio: f64,
) -> FlowState {
    let low = vol_ratio <= low_flow_vol_ratio
        && atr_ratio <= low_flow_atr_ratio
        && adx < 18.0;
    if low {
        return FlowState::Low;
    }
    let high = vol_ratio >= high_flow_vol_ratio
        && atr_ratio >= high_flow_atr_ratio
        && adx >= 18.0;
    if high {
        return FlowState::High;
    }
    FlowState::Mid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reach_long_clamps_within_minmax_band() {
        let price = 100.0;
        let atr = 10.0;
        // raw level so far away that dist = 10 ATR — must clamp to max_atr.
        let level = f_reach_long(0.0, price, atr, 0.20, 0.60);
        assert!((level - (price - atr * 0.60)).abs() < 1e-9);
        // raw level too close — must clamp to min_atr.
        let level = f_reach_long(99.0, price, atr, 0.20, 0.60);
        assert!((level - (price - atr * 0.20)).abs() < 1e-9);
    }

    #[test]
    fn reach_short_clamps_within_minmax_band() {
        let price = 100.0;
        let atr = 10.0;
        let level = f_reach_short(200.0, price, atr, 0.20, 0.60);
        assert!((level - (price + atr * 0.60)).abs() < 1e-9);
        let level = f_reach_short(101.0, price, atr, 0.20, 0.60);
        assert!((level - (price + atr * 0.20)).abs() < 1e-9);
    }

    #[test]
    fn cap_long_never_exceeds_base_plus_max_atr() {
        let cap = f_cap_long(150.0, 100.0, 10.0, 0.95);
        assert!((cap - (100.0 + 10.0 * 0.95)).abs() < 1e-9);
    }

    #[test]
    fn cap_short_never_drops_below_base_minus_max_atr() {
        let cap = f_cap_short(50.0, 100.0, 10.0, 0.95);
        assert!((cap - (100.0 - 10.0 * 0.95)).abs() < 1e-9);
    }

    #[test]
    fn long_tp_fix_enforces_step_min() {
        // raw too close to base; expect at least base + step_min*atr
        let v = f_long_tp_fix(99.5, 100.0, 99.0, 10.0, 0.22);
        assert!((v - (100.0 + 10.0 * 0.22)).abs() < 1e-9);
    }

    #[test]
    fn prob_score_monotone_in_confidence() {
        let lo = f_prob_score(40.0, 0.30, 10.0, false, false, 22.0);
        let hi = f_prob_score(80.0, 0.30, 10.0, false, false, 22.0);
        assert!(hi >= lo);
    }

    #[test]
    fn prob_score_shock_penalty_reduces() {
        let no_shock = f_prob_score(80.0, 0.30, 10.0, false, false, 22.0);
        let shocked = f_prob_score(80.0, 0.30, 10.0, false, true, 22.0);
        assert!(shocked + 25 <= no_shock, "{no_shock} vs {shocked}");
    }

    #[test]
    fn classify_flow_low_mid_high() {
        let low = classify_flow(0.5, 0.5, 10.0, 0.78, 0.82, 1.35, 1.18);
        assert_eq!(low, FlowState::Low);
        let high = classify_flow(2.0, 2.0, 25.0, 0.78, 0.82, 1.35, 1.18);
        assert_eq!(high, FlowState::High);
        let mid = classify_flow(1.0, 1.0, 22.0, 0.78, 0.82, 1.35, 1.18);
        assert_eq!(mid, FlowState::Mid);
    }
}
