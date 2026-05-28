pub mod confidence;
pub mod entry_plan;
pub mod guard_state;
pub mod mtf;
pub mod plan_context;
pub mod session;
pub mod signals;
pub mod sl_engine;
pub mod tp_engine;
pub mod trap_guard;

pub use signals::{SignalDirection, SignalResult, SignalState};

use crate::{config::AppConfig, data::proxy::ProxySnapshot, Candle};

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

    pub fn evaluate_with_mtf(
        &self,
        symbol: &str,
        candles: &[Candle],
        mtf: &mtf::MtfCandles,
    ) -> SignalResult {
        signals::SignalGenerator::new(&self.config).evaluate_with_mtf(symbol, candles, mtf)
    }

    pub fn evaluate_with_state(
        &self,
        symbol: &str,
        candles: &[Candle],
        mtf: Option<&mtf::MtfCandles>,
        proxy: Option<&ProxySnapshot>,
        prev_state: &guard_state::GuardState,
    ) -> (SignalResult, guard_state::GuardState) {
        signals::SignalGenerator::new(&self.config)
            .evaluate_with_state(symbol, candles, mtf, proxy, prev_state)
    }
}
