use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::strategy::SignalResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSnapshot {
    pub key: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSnapshot {
    pub cycle_id: String,
    pub scanned: usize,
    pub signals: usize,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSnapshot {
    pub cycle_id: String,
    pub signal: SignalResult,
    pub updated_at: DateTime<Utc>,
}
