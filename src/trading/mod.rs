pub mod executor;
pub mod order_manager;
pub mod position;
pub mod scaling;

use anyhow::Result;

use crate::{exchange::Exchange, risk::RiskManager, strategy::SignalResult};

pub struct TradingEngine<E> {
    exchange: E,
    risk: RiskManager,
}

impl<E: Exchange> TradingEngine<E> {
    pub const fn new(exchange: E, risk: RiskManager) -> Self {
        Self { exchange, risk }
    }

    pub async fn handle_signal(&self, signal: &SignalResult) -> Result<()> {
        let decision = self.risk.evaluate(signal);
        if !decision.approved {
            return Ok(());
        }

        let _ = &self.exchange;
        Ok(())
    }
}
