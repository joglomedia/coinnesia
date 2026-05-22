use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AlertJobRecord {
    pub id: Uuid,
    pub signal_id: Option<Uuid>,
    pub channel: String,
    pub status: String,
    pub payload: serde_json::Value,
    pub dedupe_key: Option<String>,
    pub scheduled_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl AlertJobRecord {
    pub fn pending(
        id: Uuid,
        signal_id: Option<Uuid>,
        channel: impl Into<String>,
        payload: serde_json::Value,
        dedupe_key: Option<String>,
        scheduled_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            signal_id,
            channel: channel.into(),
            status: "pending".to_owned(),
            payload,
            dedupe_key,
            scheduled_at,
            delivered_at: None,
            created_at: scheduled_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AlertDeliveryRecord {
    pub id: Uuid,
    pub alert_job_id: Uuid,
    pub channel: String,
    pub success: bool,
    pub provider_message_id: Option<String>,
    pub error: Option<String>,
    pub attempted_at: DateTime<Utc>,
}

impl AlertDeliveryRecord {
    pub fn new(
        id: Uuid,
        alert_job_id: Uuid,
        channel: impl Into<String>,
        success: bool,
        provider_message_id: Option<String>,
        error: Option<String>,
        attempted_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            alert_job_id,
            channel: channel.into(),
            success,
            provider_message_id,
            error,
            attempted_at,
        }
    }
}
