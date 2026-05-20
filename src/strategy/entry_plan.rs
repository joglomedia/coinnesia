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

        let ew1_center = anchor + signed * offset(self.config.ew1_min_atr);
        let ew2_center = anchor + signed * offset(self.config.ew2_atr);
        let ew3_center = anchor + signed * offset(self.config.ew3_atr);
        let deep_center = anchor + signed * offset(self.config.deep_add_atr);

        EntryPlan {
            ew1: band(ew1_center),
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
}
