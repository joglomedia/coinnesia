use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
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

    pub fn mark_passed(&self, live_trading_requested: bool) {
        let mut status = self
            .status
            .write()
            .expect("reconciliation status lock poisoned");
        status.completed = true;
        status.live_trading_unlocked = live_trading_requested;
        status.last_checked_at = Some(Utc::now());
        status.reason = if live_trading_requested {
            "startup_checks_passed_live_trading_unlocked"
        } else {
            "startup_checks_passed_no_live_trading_requested"
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
