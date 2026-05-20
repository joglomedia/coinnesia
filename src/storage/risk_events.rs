use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RiskEventRecord {
    pub id: Uuid,
    pub event_type: String,
    pub severity: String,
    pub symbol: Option<String>,
    pub approved: Option<bool>,
    pub reason: String,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct KillSwitchRecord {
    pub triggered: bool,
    pub reason: Option<String>,
    pub manual_restart_required: bool,
    pub triggered_at: Option<DateTime<Utc>>,
    pub reset_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}
