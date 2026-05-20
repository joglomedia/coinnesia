use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct FillRecord {
    pub id: Uuid,
    pub order_id: Option<Uuid>,
    pub exchange_fill_id: Option<String>,
    pub symbol: String,
    pub side: String,
    pub quantity: Decimal,
    pub price: Decimal,
    pub fee: Decimal,
    pub fee_asset: Option<String>,
    pub filled_at: DateTime<Utc>,
}
