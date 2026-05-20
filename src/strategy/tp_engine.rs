use serde::{Deserialize, Serialize};

use crate::config::EntryPlanConfig;

use super::SignalDirection;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TakeProfits {
    pub tp1: f64,
    pub tp2: f64,
    pub tp3_optional: f64,
}

impl TakeProfits {
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
        Self {
            tp1: anchor + sign * atr * config.tp1_atr.min(config.max_tp1_atr),
            tp2: anchor + sign * atr * config.tp2_atr.min(config.max_tp2_atr),
            tp3_optional: anchor + sign * atr * config.tp3_atr.min(config.max_tp3_atr),
        }
    }
}
