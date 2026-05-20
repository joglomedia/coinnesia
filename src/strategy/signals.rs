use serde::{Deserialize, Serialize};

use crate::{config::AppConfig, indicators::regime::MarketRegime, Candle};

use super::{
    confidence::ConfidenceScore,
    entry_plan::{EntryPlan, EntryPlanCalculator},
    trap_guard::TrapDecision,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SignalState {
    Long,
    Short,
    Wait,
    Freeze,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalDirection {
    Long,
    Short,
    Wait,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalResult {
    pub symbol: String,
    pub state: SignalState,
    pub confidence: ConfidenceScore,
    pub reason: String,
    pub entry_plan: Option<EntryPlan>,
}

pub struct SignalGenerator<'a> {
    config: &'a AppConfig,
}

impl<'a> SignalGenerator<'a> {
    pub const fn new(config: &'a AppConfig) -> Self {
        Self { config }
    }

    pub fn evaluate(&self, symbol: &str, candles: &[Candle]) -> SignalResult {
        if candles.len() < self.config.indicators.atr_length + 1 {
            return SignalResult::wait(symbol, "not_enough_candles");
        }

        let latest = candles.last().expect("len checked");
        let regime = classify_minimal_regime(latest);
        if regime == MarketRegime::Shock {
            return SignalResult::freeze(symbol, "shock_regime");
        }

        let trap = TrapDecision::allow();
        let confidence = ConfidenceScore::neutral();
        if trap.blocks_signal {
            return SignalResult::wait(symbol, "trap_guard_blocked");
        }

        SignalResult {
            symbol: symbol.to_owned(),
            state: SignalState::Wait,
            confidence,
            reason: "strategy_layers_not_implemented_yet".to_owned(),
            entry_plan: None,
        }
    }
}

impl SignalResult {
    pub fn wait(symbol: &str, reason: impl Into<String>) -> Self {
        Self {
            symbol: symbol.to_owned(),
            state: SignalState::Wait,
            confidence: ConfidenceScore::neutral(),
            reason: reason.into(),
            entry_plan: None,
        }
    }

    pub fn freeze(symbol: &str, reason: impl Into<String>) -> Self {
        Self {
            symbol: symbol.to_owned(),
            state: SignalState::Freeze,
            confidence: ConfidenceScore::neutral(),
            reason: reason.into(),
            entry_plan: None,
        }
    }

    pub fn with_entry_plan(mut self, config: &AppConfig, anchor: f64, atr: f64) -> Self {
        self.entry_plan = Some(EntryPlanCalculator::new(&config.entry_plan).calculate(
            self.state.into(),
            anchor,
            atr,
        ));
        self
    }
}

impl From<SignalState> for SignalDirection {
    fn from(value: SignalState) -> Self {
        match value {
            SignalState::Long => Self::Long,
            SignalState::Short => Self::Short,
            SignalState::Wait | SignalState::Freeze => Self::Wait,
        }
    }
}

fn classify_minimal_regime(candle: &Candle) -> MarketRegime {
    if candle.high < candle.low || candle.open <= 0.0 || candle.close <= 0.0 {
        MarketRegime::Shock
    } else {
        MarketRegime::Sideways
    }
}
