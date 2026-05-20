use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AlertJobRecord {
    pub id: Uuid,
    pub signal_id: Option<Uuid>,
    pub channel: String,
    pub status: String,
}
