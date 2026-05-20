pub mod confidence;
pub mod entry_plan;
pub mod mtf;
pub mod session;
pub mod signals;
pub mod sl_engine;
pub mod tp_engine;
pub mod trap_guard;

pub use signals::{SignalDirection, SignalResult, SignalState};

use crate::{config::AppConfig, Candle};

pub struct StrategyEngine {
    config: AppConfig,
}

impl StrategyEngine {
    pub const fn new(config: AppConfig) -> Self {
        Self { config }
    }

    pub fn evaluate(&self, symbol: &str, candles: &[Candle]) -> SignalResult {
        signals::SignalGenerator::new(&self.config).evaluate(symbol, candles)
    }
}
