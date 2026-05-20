use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct BalanceSnapshotRecord {
    pub id: Uuid,
    pub asset: String,
    pub available: Decimal,
    pub locked: Decimal,
    pub captured_at: DateTime<Utc>,
}
