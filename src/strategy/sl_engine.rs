use serde::{Deserialize, Serialize};

use crate::config::EntryPlanConfig;

use super::SignalDirection;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StopLoss {
    pub price: f64,
    pub atr_distance: f64,
}

impl StopLoss {
    pub fn from_atr(
        direction: SignalDirection,
        anchor: f64,
        atr: f64,
        config: &EntryPlanConfig,
    ) -> Self {
        let atr_distance = config.min_sl_distance_atr.max(0.0);
        let sign = match direction {
            SignalDirection::Long => -1.0,
            SignalDirection::Short => 1.0,
            SignalDirection::Wait => 0.0,
        };
        Self {
            price: anchor + sign * atr * atr_distance,
            atr_distance,
        }
    }
}
