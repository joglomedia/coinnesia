use std::sync::Arc;

use serde::Serialize;

use crate::{
    app::reconciliation::ReconciliationStatus, config::AppConfig, observability::HealthSnapshot,
};

#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub healthy: bool,
    pub components: Vec<ComponentHealth>,
    pub reconciliation: Option<ReconciliationStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
    pub name: String,
    pub healthy: bool,
}

impl HealthResponse {
    pub fn from_snapshot(snapshot: HealthSnapshot) -> Self {
        Self {
            healthy: snapshot.healthy,
            components: snapshot
                .components
                .into_iter()
                .map(|component| ComponentHealth {
                    name: component.name,
                    healthy: component.healthy,
                })
                .collect(),
            reconciliation: None,
        }
    }

    pub fn with_reconciliation(mut self, status: ReconciliationStatus) -> Self {
        self.reconciliation = Some(status);
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigSummary {
    pub symbols: usize,
    pub exchange: String,
    pub trading_mode: String,
    pub server_enabled: bool,
    pub database_enabled: bool,
    pub cache_enabled: bool,
}

impl ConfigSummary {
    pub fn from_config(config: &Arc<AppConfig>) -> Self {
        Self {
            symbols: config.symbols.len(),
            exchange: config.exchange.platform.clone(),
            trading_mode: config.trading.mode.clone(),
            server_enabled: config.server.enabled,
            database_enabled: config.database.enabled,
            cache_enabled: config.cache.enabled,
        }
    }
}
