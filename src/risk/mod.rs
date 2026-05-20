pub mod correlation;
pub mod drawdown;
pub mod kill_switch;
pub mod limits;
pub mod position_sizer;

use crate::{config::RiskConfig, strategy::SignalResult};

#[derive(Debug, Clone, PartialEq)]
pub struct RiskDecision {
    pub approved: bool,
    pub reason: String,
}

pub struct RiskManager {
    config: RiskConfig,
}

impl RiskManager {
    pub const fn new(config: RiskConfig) -> Self {
        Self { config }
    }

    pub fn evaluate(&self, signal: &SignalResult) -> RiskDecision {
        if self.config.kill_switch.enabled
            && matches!(signal.state, crate::strategy::SignalState::Freeze)
        {
            return RiskDecision {
                approved: false,
                reason: "kill_switch_freeze_guard".to_owned(),
            };
        }

        RiskDecision {
            approved: true,
            reason: "approved".to_owned(),
        }
    }
}
