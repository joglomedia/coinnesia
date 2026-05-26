use serde::{Deserialize, Serialize};

use crate::{
    config::{EntryPlanConfig, TrapGuardConfig},
    Candle,
};

use super::{
    plan_context::{f_reach_long, f_reach_short, PlanContext},
    sl_engine::StopLoss,
    tp_engine::TakeProfits,
    SignalDirection,
};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PriceBand {
    pub low: f64,
    pub high: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryPlan {
    pub ew1: PriceBand,
    pub ew2: PriceBand,
    pub ew3: PriceBand,
    pub deep_add: PriceBand,
    pub entry_ideal: f64,
    pub take_profits: TakeProfits,
    pub stop_loss: StopLoss,
}

pub struct EntryPlanCalculator<'a> {
    config: &'a EntryPlanConfig,
}

impl<'a> EntryPlanCalculator<'a> {
    pub const fn new(config: &'a EntryPlanConfig) -> Self {
        Self { config }
    }

    /// Legacy ATR-only path retained for callers that don't yet have a
    /// PlanContext. Produces a Pine-shaped plan but with raw `close` as the
    /// swing anchor and no flow/probability scoring.
    pub fn calculate(&self, direction: SignalDirection, anchor: f64, atr: f64) -> EntryPlan {
        let offset = |multiple: f64| atr * multiple;
        let width = offset(self.config.entry_zone_atr);

        let band = |center: f64| PriceBand {
            low: center - width,
            high: center + width,
        };

        let signed = match direction {
            SignalDirection::Long => -1.0,
            SignalDirection::Short => 1.0,
            SignalDirection::Wait => 0.0,
        };

        let ew1_near = anchor + signed * offset(self.config.ew1_min_atr);
        let ew1_far = anchor + signed * offset(self.config.ew1_max_atr);
        let ew1 = PriceBand {
            low: ew1_near.min(ew1_far),
            high: ew1_near.max(ew1_far),
        };
        let ew2_center = anchor + signed * offset(self.config.ew2_atr);
        let ew3_center = anchor + signed * offset(self.config.ew3_atr);
        let deep_center = anchor + signed * offset(self.config.deep_add_atr);

        // Weighted EW anchor (Pine "ENTRY IDEAL"): 0.10*EW1 + 0.35*EW2 + 0.55*EW3.
        let ew1_mid = (ew1.low + ew1.high) * 0.5;
        let entry_ideal = ew1_mid * 0.10 + ew2_center * 0.35 + ew3_center * 0.55;

        EntryPlan {
            ew1,
            ew2: band(ew2_center),
            ew3: band(ew3_center),
            deep_add: band(deep_center),
            entry_ideal,
            take_profits: TakeProfits::from_atr(direction, anchor, atr, self.config),
            stop_loss: StopLoss::from_atr(direction, anchor, atr, self.config),
        }
    }

    /// Pine V61.9 EW engine: derives EW1 from `rawAnchor = max(swingLow,
    /// dailyLow, close - ATR*ew1MaxEff)` (long; mirror for short), then
    /// clamps via `f_reach_*`. EW2/EW3/Deep extend in fixed ATR steps with
    /// minimum spacing.
    pub fn calculate_from_context(
        &self,
        ctx: &PlanContext,
        candles: &[Candle],
        trap_guard: &TrapGuardConfig,
    ) -> EntryPlan {
        let atr = ctx.atr.max(0.0);
        let zone_width = atr * self.config.entry_zone_atr;
        let band = |center: f64| PriceBand {
            low: center - zone_width,
            high: center + zone_width,
        };

        let ew1_max_eff = self.config.ew1_max_atr;
        let ew2_eff = self.config.ew2_atr;
        let ew3_eff = self.config.ew3_atr;
        let deep_eff = self.config.deep_add_atr;

        match ctx.direction {
            SignalDirection::Long => {
                let raw_anchor = ctx
                    .swing_low
                    .max(ctx.daily_low)
                    .max(ctx.close - atr * ew1_max_eff);
                let ew1_price = f_reach_long(
                    raw_anchor,
                    ctx.close,
                    atr,
                    self.config.ew1_min_atr,
                    ew1_max_eff,
                );
                let ew2_price = (ew1_price - atr * ew2_eff)
                    .min(ctx.close - atr * (self.config.ew1_min_atr + 0.10));
                let ew3_price = (ew1_price - atr * ew3_eff).min(ew2_price - atr * 0.08);
                let deep_price = (ew1_price - atr * deep_eff).min(ew3_price - atr * 0.08);

                let ew1_band = PriceBand {
                    low: ew1_price - zone_width,
                    high: ew1_price + zone_width,
                };
                let ew2_band = band(ew2_price);
                let ew3_band = band(ew3_price);
                let deep_band = band(deep_price);

                let entry_ideal = ew1_price * 0.10 + ew2_price * 0.35 + ew3_price * 0.55;

                let stop_loss = StopLoss::from_context(
                    ctx,
                    candles,
                    self.config,
                    trap_guard,
                    deep_price,
                    ew1_price,
                    ew3_price,
                );
                let take_profits = TakeProfits::from_context(ctx, self.config, ew1_price);

                EntryPlan {
                    ew1: ew1_band,
                    ew2: ew2_band,
                    ew3: ew3_band,
                    deep_add: deep_band,
                    entry_ideal,
                    take_profits,
                    stop_loss,
                }
            }
            SignalDirection::Short => {
                let raw_anchor = ctx
                    .swing_high
                    .min(ctx.daily_high)
                    .min(ctx.close + atr * ew1_max_eff);
                let ew1_price = f_reach_short(
                    raw_anchor,
                    ctx.close,
                    atr,
                    self.config.ew1_min_atr,
                    ew1_max_eff,
                );
                let ew2_price = (ew1_price + atr * ew2_eff)
                    .max(ctx.close + atr * (self.config.ew1_min_atr + 0.10));
                let ew3_price = (ew1_price + atr * ew3_eff).max(ew2_price + atr * 0.08);
                let deep_price = (ew1_price + atr * deep_eff).max(ew3_price + atr * 0.08);

                let ew1_band = PriceBand {
                    low: ew1_price - zone_width,
                    high: ew1_price + zone_width,
                };
                let ew2_band = band(ew2_price);
                let ew3_band = band(ew3_price);
                let deep_band = band(deep_price);

                let entry_ideal = ew1_price * 0.10 + ew2_price * 0.35 + ew3_price * 0.55;

                let stop_loss = StopLoss::from_context(
                    ctx,
                    candles,
                    self.config,
                    trap_guard,
                    deep_price,
                    ew1_price,
                    ew3_price,
                );
                let take_profits = TakeProfits::from_context(ctx, self.config, ew1_price);

                EntryPlan {
                    ew1: ew1_band,
                    ew2: ew2_band,
                    ew3: ew3_band,
                    deep_add: deep_band,
                    entry_ideal,
                    take_profits,
                    stop_loss,
                }
            }
            SignalDirection::Wait => {
                self.calculate(SignalDirection::Wait, ctx.close, atr)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use crate::{
        config::AppConfig,
        indicators::regime::MarketRegime,
        strategy::{
            plan_context::{FlowState, PlanContext},
            session::MarketSession,
        },
        Candle,
    };

    use super::*;

    fn flat_candles(n: usize) -> Vec<Candle> {
        let now = Utc::now();
        (0..n)
            .map(|idx| Candle {
                ts: now + Duration::minutes(idx as i64),
                open: 100.0,
                high: 102.0,
                low: 98.0,
                close: 100.0,
                volume: 1_000.0,
            })
            .collect()
    }

    fn long_ctx(swing_low: f64, atr: f64) -> PlanContext {
        PlanContext {
            direction: SignalDirection::Long,
            close: 100.0,
            atr,
            adx: 25.0,
            confidence: 70.0,
            trap_score: 0.0,
            upper_wick: 0.0,
            lower_wick: 0.0,
            swing_high: 102.0,
            swing_low,
            daily_high: 102.0,
            daily_low: swing_low,
            weekly_high: 105.0,
            weekly_low: swing_low,
            vwap: 99.5,
            ema_fast: 99.8,
            ema_slow: 99.0,
            session: MarketSession::Usa,
            regime: MarketRegime::TrendExpansion,
            flow_state: FlowState::High,
            trap_now: false,
            cooldown_active: false,
            vol_shock: false,
            shock_active: false,
        }
    }

    #[test]
    fn entry_plan_uses_atr_distances() {
        let config = AppConfig::from_default_toml().unwrap();
        let plan = EntryPlanCalculator::new(&config.entry_plan).calculate(
            SignalDirection::Long,
            100.0,
            10.0,
        );
        assert!(plan.ew2.high < plan.ew1.high);
        assert!(plan.stop_loss.price < 100.0);
    }

    #[test]
    fn ew1_band_spans_min_to_max_atr() {
        let config = AppConfig::from_default_toml().unwrap();
        let atr = 10.0;
        let anchor = 100.0;
        let plan = EntryPlanCalculator::new(&config.entry_plan).calculate(
            SignalDirection::Long,
            anchor,
            atr,
        );
        let expected_high = anchor - config.entry_plan.ew1_min_atr * atr;
        let expected_low = anchor - config.entry_plan.ew1_max_atr * atr;
        assert!((plan.ew1.high - expected_high).abs() < 1e-6, "{:?}", plan.ew1);
        assert!((plan.ew1.low - expected_low).abs() < 1e-6, "{:?}", plan.ew1);
        let expected_width = (config.entry_plan.ew1_max_atr - config.entry_plan.ew1_min_atr) * atr;
        assert!(
            (plan.ew1.high - plan.ew1.low - expected_width).abs() < 1e-6,
            "EW1 band width must equal (max - min) * ATR; got {:?}",
            plan.ew1,
        );
    }

    #[test]
    fn context_plan_orders_ew_levels_for_long() {
        let config = AppConfig::from_default_toml().unwrap();
        let ctx = long_ctx(99.5, 1.0);
        let plan = EntryPlanCalculator::new(&config.entry_plan).calculate_from_context(
            &ctx,
            &flat_candles(60),
            &config.trap_guard,
        );
        // Pine ordering: EW1 ≥ EW2 ≥ EW3 ≥ DEEP for a long setup.
        let ew1_mid = (plan.ew1.low + plan.ew1.high) * 0.5;
        let ew2_mid = (plan.ew2.low + plan.ew2.high) * 0.5;
        let ew3_mid = (plan.ew3.low + plan.ew3.high) * 0.5;
        let deep_mid = (plan.deep_add.low + plan.deep_add.high) * 0.5;
        assert!(ew1_mid > ew2_mid, "ew1 should be above ew2: {ew1_mid} vs {ew2_mid}");
        assert!(ew2_mid > ew3_mid);
        assert!(ew3_mid > deep_mid);
        assert!(plan.stop_loss.price < deep_mid);
    }

    #[test]
    fn context_plan_marks_setup_rejected_when_swing_too_far() {
        let mut config = AppConfig::from_default_toml().unwrap();
        // Force a tight max-SL cap for USA so the structural stop trips reject.
        config.entry_plan.max_sl_usa_atr = 0.50;
        // Place swingLow 5 ATR below close → SL falls far away.
        let ctx = long_ctx(90.0, 1.0);
        let plan = EntryPlanCalculator::new(&config.entry_plan).calculate_from_context(
            &ctx,
            &flat_candles(60),
            &config.trap_guard,
        );
        assert!(
            plan.stop_loss.rejected_too_wide,
            "expected setup rejection — raw distance must exceed max_sl_usa_atr"
        );
    }

    #[test]
    fn context_plan_high_flow_allows_full_tp_caps() {
        let config = AppConfig::from_default_toml().unwrap();
        let mut ctx = long_ctx(99.5, 1.0);
        ctx.flow_state = FlowState::High;
        let plan_high = EntryPlanCalculator::new(&config.entry_plan).calculate_from_context(
            &ctx,
            &flat_candles(60),
            &config.trap_guard,
        );
        ctx.flow_state = FlowState::Low;
        let plan_low = EntryPlanCalculator::new(&config.entry_plan).calculate_from_context(
            &ctx,
            &flat_candles(60),
            &config.trap_guard,
        );
        // Low-flow caps must squeeze TP3 closer to base than high-flow.
        assert!(
            plan_low.take_profits.tp3_optional <= plan_high.take_profits.tp3_optional + 1e-9,
            "low-flow TP3 must be ≤ high-flow TP3: low={} high={}",
            plan_low.take_profits.tp3_optional,
            plan_high.take_profits.tp3_optional
        );
    }

    #[test]
    fn context_plan_attaches_probability_and_labels() {
        let config = AppConfig::from_default_toml().unwrap();
        let ctx = long_ctx(99.5, 1.0);
        let plan = EntryPlanCalculator::new(&config.entry_plan).calculate_from_context(
            &ctx,
            &flat_candles(60),
            &config.trap_guard,
        );
        assert!(plan.take_profits.tp1_prob <= 100);
        assert!(plan.take_profits.tp1_prob >= plan.take_profits.tp2_prob);
        assert!(plan.take_profits.tp2_prob >= plan.take_profits.tp3_prob);
    }
}
