use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RiskEventRecord {
    pub id: Uuid,
    pub reason: String,
}
