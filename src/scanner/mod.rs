pub mod rate_limiter;

use anyhow::Result;
use futures::future::join_all;
use tracing::info;

use crate::{config::AppConfig, strategy::SignalResult};

#[derive(Debug, Clone)]
pub struct ScanReport {
    pub scanned: usize,
    pub signals: Vec<SignalResult>,
}

pub struct Scanner {
    config: AppConfig,
}

impl Scanner {
    pub const fn new(config: AppConfig) -> Self {
        Self { config }
    }

    pub async fn scan_once(&self) -> Result<ScanReport> {
        let strategy_config = self.config.clone();
        let tasks = self.config.symbols.iter().map(|symbol| {
            let strategy_config = strategy_config.clone();
            let symbol = symbol.symbol.clone();
            tokio::spawn(async move {
                let engine = crate::strategy::StrategyEngine::new(strategy_config);
                engine.evaluate(&symbol, &[])
            })
        });

        let results = join_all(tasks).await;
        let signals = results
            .into_iter()
            .filter_map(|result| result.ok())
            .collect::<Vec<_>>();

        Ok(ScanReport {
            scanned: self.config.symbols.len(),
            signals,
        })
    }

    pub async fn run(&self) -> Result<()> {
        let report = self.scan_once().await?;
        info!(
            scanned = report.scanned,
            signals = report.signals.len(),
            "scanner loop placeholder completed one cycle"
        );
        Ok(())
    }
}
