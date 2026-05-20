use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct RuntimeSnapshot {
    pub key: String,
    pub updated_at: DateTime<Utc>,
}
