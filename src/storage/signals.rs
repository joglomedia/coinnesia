use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SignalRecord {
    pub id: Uuid,
    pub symbol: String,
    pub timeframe: Option<String>,
    pub asset_class: Option<String>,
    pub state: String,
    pub direction: String,
    pub confidence: Decimal,
    pub directional_gap: Decimal,
    pub reason: String,
    pub entry_plan: Option<serde_json::Value>,
    pub indicators: serde_json::Value,
    pub evaluated_at: DateTime<Utc>,
}

impl SignalRecord {
    pub fn new(
        id: Uuid,
        symbol: impl Into<String>,
        state: impl Into<String>,
        direction: impl Into<String>,
        confidence: Decimal,
        directional_gap: Decimal,
        reason: impl Into<String>,
        evaluated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            symbol: symbol.into(),
            timeframe: None,
            asset_class: None,
            state: state.into(),
            direction: direction.into(),
            confidence,
            directional_gap,
            reason: reason.into(),
            entry_plan: None,
            indicators: serde_json::json!({}),
            evaluated_at,
        }
    }
}
