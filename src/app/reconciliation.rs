use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationStatus {
    pub completed: bool,
    pub live_trading_unlocked: bool,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub reason: String,
}

impl ReconciliationStatus {
    pub fn pending() -> Self {
        Self {
            completed: false,
            live_trading_unlocked: false,
            last_checked_at: None,
            reason: "not_run".to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StartupGate {
    status: Arc<RwLock<ReconciliationStatus>>,
}

impl StartupGate {
    pub fn pending() -> Self {
        Self {
            status: Arc::new(RwLock::new(ReconciliationStatus::pending())),
        }
    }

    pub fn status(&self) -> ReconciliationStatus {
        self.status
            .read()
            .expect("reconciliation status lock poisoned")
            .clone()
    }

    pub fn mark_passed(&self, live_trading_unlocked: bool) {
        let mut status = self
            .status
            .write()
            .expect("reconciliation status lock poisoned");
        status.completed = true;
        status.live_trading_unlocked = live_trading_unlocked;
        status.last_checked_at = Some(Utc::now());
        status.reason = if live_trading_unlocked {
            "startup_checks_passed_live_trading_unlocked"
        } else {
            "startup_checks_passed_trading_locked"
        }
        .to_owned();
    }

    pub fn mark_failed(&self, reason: impl Into<String>) {
        let mut status = self
            .status
            .write()
            .expect("reconciliation status lock poisoned");
        status.completed = true;
        status.live_trading_unlocked = false;
        status.last_checked_at = Some(Utc::now());
        status.reason = reason.into();
    }

    pub fn live_trading_unlocked(&self) -> bool {
        self.status().live_trading_unlocked
    }
}

impl Default for StartupGate {
    fn default() -> Self {
        Self::pending()
    }
}

pub fn can_unlock_live_trading(config: &AppConfig) -> bool {
    config.trading.enabled
        && config.trading.mode == "live"
        && config.database.enabled
        && config.cache.enabled
        && config.risk.kill_switch.enabled
}

#[cfg(test)]
mod tests {
    use crate::config::AppConfig;

    use super::can_unlock_live_trading;

    #[test]
    fn live_trading_gate_requires_database_cache_and_kill_switch() {
        let mut config = AppConfig::from_default_toml().expect("default config parses");
        config.trading.enabled = true;
        config.trading.mode = "live".to_owned();
        config.database.enabled = true;
        config.cache.enabled = true;
        config.risk.kill_switch.enabled = true;
        assert!(can_unlock_live_trading(&config));

        config.cache.enabled = false;
        assert!(!can_unlock_live_trading(&config));
    }
}
