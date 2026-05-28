use serde::{Deserialize, Serialize};

use crate::config::EntryPlanConfig;

use super::{
    plan_context::{
        f_cap_long, f_cap_short, f_long_tp_fix, f_long_tp_flow, f_prob_score, f_short_tp_fix,
        f_short_tp_flow, FlowState, PlanContext,
    },
    SignalDirection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ProbLabel {
    LowProb,
    OkProb,
    HighProb,
}

impl ProbLabel {
    pub fn from_score(prob: u32, min_tp1: u32, min_tp2: u32) -> Self {
        if prob >= min_tp1 {
            Self::HighProb
        } else if prob >= min_tp2 {
            Self::OkProb
        } else {
            Self::LowProb
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::LowProb => "LOW PROB",
            Self::OkProb => "OK PROB",
            Self::HighProb => "HIGH PROB",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TakeProfitLevel {
    pub price: f64,
    pub probability: u32,
    pub label: ProbLabel,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TakeProfits {
    pub tp1: f64,
    pub tp2: f64,
    pub tp3_optional: f64,
    pub tp1_prob: u32,
    pub tp2_prob: u32,
    pub tp3_prob: u32,
    pub tp1_label: ProbLabel,
    pub tp2_label: ProbLabel,
    pub tp3_label: ProbLabel,
}

impl TakeProfits {
    /// Legacy ATR-only constructor retained for callers that have not yet
    /// migrated to `from_context`. Produces probability defaults of 50/35/25
    /// so callers can still differentiate by label without true scoring.
    pub fn from_atr(
        direction: SignalDirection,
        anchor: f64,
        atr: f64,
        config: &EntryPlanConfig,
    ) -> Self {
        let sign = match direction {
            SignalDirection::Long => 1.0,
            SignalDirection::Short => -1.0,
            SignalDirection::Wait => 0.0,
        };
        let probs = (50_u32, 35_u32, 25_u32);
        let labels = (
            ProbLabel::from_score(
                probs.0,
                config.probability.min_tp1_prob,
                config.probability.min_tp2_prob,
            ),
            ProbLabel::from_score(
                probs.1,
                config.probability.min_tp1_prob,
                config.probability.min_tp2_prob,
            ),
            ProbLabel::from_score(
                probs.2,
                config.probability.min_tp1_prob,
                config.probability.min_tp2_prob,
            ),
        );
        Self {
            tp1: anchor + sign * atr * config.tp1_atr.min(config.max_tp1_atr),
            tp2: anchor + sign * atr * config.tp2_atr.min(config.max_tp2_atr),
            tp3_optional: anchor + sign * atr * config.tp3_atr.min(config.max_tp3_atr),
            tp1_prob: probs.0,
            tp2_prob: probs.1,
            tp3_prob: probs.2,
            tp1_label: labels.0,
            tp2_label: labels.1,
            tp3_label: labels.2,
        }
    }

    /// Port of Pine V61.9 TP engine. Computes TP1/2/3 with trend factor,
    /// session factor, liquidity caps (daily/weekly), V61.8 flow-adaptive
    /// caps, and per-TP probability via `f_prob_score`.
    pub fn from_context(ctx: &PlanContext, config: &EntryPlanConfig, ew1: f64) -> Self {
        let atr = ctx.atr.max(0.0);
        let sideways = ctx.is_sideways();
        let trend_factor = if sideways {
            0.62
        } else if ctx.adx >= 25.0 {
            0.95
        } else {
            0.78
        };
        let session_factor = match ctx.session {
            super::session::MarketSession::Asia => 0.88,
            super::session::MarketSession::Europe => 1.05,
            super::session::MarketSession::Usa => 1.08,
            super::session::MarketSession::Idx => 0.95,
            super::session::MarketSession::RolloverAvoid => 0.85,
        };
        let step_min = config.tp_step_min_atr;

        let (max_tp1, max_tp2, max_tp3) = if sideways {
            (
                config.max_tp1_atr.min(0.60),
                config.max_tp2_atr.min(0.95),
                config.max_tp3_atr.min(1.30),
            )
        } else {
            (config.max_tp1_atr, config.max_tp2_atr, config.max_tp3_atr)
        };

        // Pine `flowTrapBlock` forces low-flow caps regardless of the
        // detected `flowState` so TP1/2/3 don't chase past stalling
        // liquidity when a trap is co-firing with thin volume.
        let effective_flow = if ctx.flow_trap_block {
            FlowState::Low
        } else {
            ctx.flow_state
        };
        let (flow_max_tp1, flow_max_tp2, flow_max_tp3, flow_min_step) = match effective_flow {
            FlowState::Low => (
                config.flow.low_liq_tp1_max_atr,
                config.flow.low_liq_tp2_max_atr,
                config.flow.low_liq_tp3_max_atr,
                step_min.min(config.flow.low_liq_min_step_atr),
            ),
            FlowState::Mid => (
                config.flow.mid_liq_tp1_max_atr,
                config.flow.mid_liq_tp2_max_atr,
                config.flow.mid_liq_tp3_max_atr,
                step_min,
            ),
            FlowState::High => (max_tp1, max_tp2, max_tp3, step_min),
        };

        match ctx.direction {
            SignalDirection::Long => {
                let base = ctx.close.max(ew1);
                let liq_cap = (if ctx.daily_high > base {
                    ctx.daily_high
                } else {
                    base + atr * config.tp3_atr
                })
                .min(if ctx.weekly_high > base {
                    ctx.weekly_high
                } else {
                    base + atr * config.tp3_atr
                });
                let raw1 = base + atr * config.tp1_atr * trend_factor * session_factor;
                let raw2 = base + atr * config.tp2_atr * trend_factor * session_factor;
                let raw3 = base + atr * config.tp3_atr * trend_factor * session_factor;
                let pre1 = raw1.min(liq_cap.max(base + atr * step_min));
                let tp1 = f_cap_long(
                    f_long_tp_fix(pre1, base, base, atr, step_min),
                    base,
                    atr,
                    max_tp1,
                );
                let pre2 = raw2.min(liq_cap.max(tp1 + atr * step_min));
                let tp2 = f_cap_long(
                    f_long_tp_fix(pre2, base, tp1, atr, step_min),
                    base,
                    atr,
                    max_tp2,
                );
                let pre3 = raw3.min(liq_cap.max(tp2 + atr * step_min));
                let tp3 = f_cap_long(
                    f_long_tp_fix(pre3, base, tp2, atr, step_min),
                    base,
                    atr,
                    max_tp3,
                );
                let tp1 = f_long_tp_flow(tp1, base, base, atr, flow_max_tp1, flow_min_step);
                let tp2 = f_long_tp_flow(tp2, base, tp1, atr, flow_max_tp2, flow_min_step);
                let tp3 = f_long_tp_flow(tp3, base, tp2, atr, flow_max_tp3, flow_min_step);

                let probs = compute_probs(ctx, config, tp1, tp2, tp3, atr);
                build(tp1, tp2, tp3, probs, config)
            }
            SignalDirection::Short => {
                let base = ctx.close.min(ew1);
                let liq_cap = (if ctx.daily_low < base {
                    ctx.daily_low
                } else {
                    base - atr * config.tp3_atr
                })
                .max(if ctx.weekly_low < base {
                    ctx.weekly_low
                } else {
                    base - atr * config.tp3_atr
                });
                let raw1 = base - atr * config.tp1_atr * trend_factor * session_factor;
                let raw2 = base - atr * config.tp2_atr * trend_factor * session_factor;
                let raw3 = base - atr * config.tp3_atr * trend_factor * session_factor;
                let pre1 = raw1.max(liq_cap.min(base - atr * step_min));
                let tp1 = f_cap_short(
                    f_short_tp_fix(pre1, base, base, atr, step_min),
                    base,
                    atr,
                    max_tp1,
                );
                let pre2 = raw2.max(liq_cap.min(tp1 - atr * step_min));
                let tp2 = f_cap_short(
                    f_short_tp_fix(pre2, base, tp1, atr, step_min),
                    base,
                    atr,
                    max_tp2,
                );
                let pre3 = raw3.max(liq_cap.min(tp2 - atr * step_min));
                let tp3 = f_cap_short(
                    f_short_tp_fix(pre3, base, tp2, atr, step_min),
                    base,
                    atr,
                    max_tp3,
                );
                let tp1 = f_short_tp_flow(tp1, base, base, atr, flow_max_tp1, flow_min_step);
                let tp2 = f_short_tp_flow(tp2, base, tp1, atr, flow_max_tp2, flow_min_step);
                let tp3 = f_short_tp_flow(tp3, base, tp2, atr, flow_max_tp3, flow_min_step);

                let probs = compute_probs(ctx, config, tp1, tp2, tp3, atr);
                build(tp1, tp2, tp3, probs, config)
            }
            SignalDirection::Wait => Self::from_atr(SignalDirection::Wait, ctx.close, atr, config),
        }
    }
}

fn compute_probs(
    ctx: &PlanContext,
    config: &EntryPlanConfig,
    tp1: f64,
    tp2: f64,
    tp3: f64,
    atr: f64,
) -> (u32, u32, u32) {
    let safe_atr = atr.max(f64::EPSILON);
    let dist1 = (tp1 - ctx.close).abs() / safe_atr;
    let _dist2 = (tp2 - ctx.close).abs() / safe_atr;
    let _dist3 = (tp3 - ctx.close).abs() / safe_atr;
    let sideways = ctx.is_sideways();
    let flow_penalty = match ctx.flow_state {
        FlowState::Low => config.flow.low_flow_prob_penalty,
        FlowState::Mid => 4.0,
        FlowState::High => 0.0,
    };

    let prob1_raw = f_prob_score(
        ctx.confidence,
        dist1,
        ctx.trap_score,
        sideways,
        ctx.shock_active,
        ctx.adx,
    ) as f64
        - flow_penalty;
    let prob1 = prob1_raw.clamp(0.0, 100.0).round() as u32;
    // Pine: prob2 = prob1 - 18 (- sideways 8 - low flow 6)
    let prob2_raw = prob1 as f64 - 18.0
        - if sideways { 8.0 } else { 0.0 }
        - if matches!(ctx.flow_state, FlowState::Low) {
            6.0
        } else {
            0.0
        };
    let prob2 = prob2_raw.clamp(0.0, 100.0).round() as u32;
    // Pine: prob3 = prob2 - (sideways ? 30 : 22) - (lowFlow ? 8 : 0)
    let prob3_raw = prob2 as f64
        - if sideways { 30.0 } else { 22.0 }
        - if matches!(ctx.flow_state, FlowState::Low) {
            8.0
        } else {
            0.0
        };
    let prob3 = prob3_raw.clamp(0.0, 100.0).round() as u32;
    (prob1, prob2, prob3)
}

fn build(
    tp1: f64,
    tp2: f64,
    tp3: f64,
    probs: (u32, u32, u32),
    config: &EntryPlanConfig,
) -> TakeProfits {
    let min1 = config.probability.min_tp1_prob;
    let min2 = config.probability.min_tp2_prob;
    TakeProfits {
        tp1,
        tp2,
        tp3_optional: tp3,
        tp1_prob: probs.0,
        tp2_prob: probs.1,
        tp3_prob: probs.2,
        tp1_label: ProbLabel::from_score(probs.0, min1, min2),
        tp2_label: ProbLabel::from_score(probs.1, min1, min2),
        tp3_label: ProbLabel::from_score(probs.2, min1, min2),
    }
}
