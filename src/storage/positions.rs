use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PositionRecord {
    pub id: Uuid,
    pub symbol: String,
    pub asset_class: String,
    pub side: String,
    pub status: String,
    pub quantity: Decimal,
    pub average_entry: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}
