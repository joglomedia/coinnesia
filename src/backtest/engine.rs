use anyhow::Result;
use tracing::info;

use crate::config::AppConfig;

pub struct BacktestEngine {
    config: AppConfig,
}

impl BacktestEngine {
    pub const fn new(config: AppConfig) -> Self {
        Self { config }
    }

    pub async fn run(&self) -> Result<()> {
        info!(
            enabled = self.config.backtest.enabled,
            start = %self.config.backtest.start_date,
            end = %self.config.backtest.end_date,
            "backtest engine initialized"
        );
        Ok(())
    }
}
