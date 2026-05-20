use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SignalRecord {
    pub id: Uuid,
    pub symbol: String,
}
