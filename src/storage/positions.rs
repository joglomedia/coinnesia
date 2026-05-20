use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PositionRecord {
    pub id: Uuid,
    pub symbol: String,
}
