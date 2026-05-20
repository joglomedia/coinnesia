use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct OrderRecord {
    pub id: Uuid,
    pub symbol: String,
}
