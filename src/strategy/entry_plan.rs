use serde::{Deserialize, Serialize};

use crate::config::EntryPlanConfig;

use super::{sl_engine::StopLoss, tp_engine::TakeProfits, SignalDirection};

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

        // EW1 is a reachability band spanning min..max ATR from anchor.
        // Without a structural swing reference (1.7.5 territory), this is
        // Pine's `f_reach_*` reduced to a flat clamp between the two limits.
        let ew1_near = anchor + signed * offset(self.config.ew1_min_atr);
        let ew1_far = anchor + signed * offset(self.config.ew1_max_atr);
        let ew1 = PriceBand {
            low: ew1_near.min(ew1_far),
            high: ew1_near.max(ew1_far),
        };
        let ew2_center = anchor + signed * offset(self.config.ew2_atr);
        let ew3_center = anchor + signed * offset(self.config.ew3_atr);
        let deep_center = anchor + signed * offset(self.config.deep_add_atr);

        EntryPlan {
            ew1,
            ew2: band(ew2_center),
            ew3: band(ew3_center),
            deep_add: band(deep_center),
            take_profits: TakeProfits::from_atr(direction, anchor, atr, self.config),
            stop_loss: StopLoss::from_atr(direction, anchor, atr, self.config),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::AppConfig;

    use super::*;

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
        // Pre-fix bug: `ew1_max_atr` was dead config — EW1 was centered at
        // `ew1_min_atr` only. Pine clamps the reach distance between
        // `ew1MinATR` and `ew1MaxATR`, so the band must span both bounds.
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
}
