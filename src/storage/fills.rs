use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct FillRecord {
    pub id: Uuid,
    pub order_id: Uuid,
}
