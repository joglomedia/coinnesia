use std::time::Duration;

use anyhow::{Context, Result};

use super::Cache;

#[derive(Debug, Clone, Copy)]
pub struct RateLimitBucket {
    pub capacity: usize,
    pub window: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitDecision {
    pub allowed: bool,
    pub capacity: usize,
    pub current: usize,
    pub remaining: usize,
}

impl RateLimitBucket {
    pub const fn new(capacity: usize, window: Duration) -> Self {
        Self { capacity, window }
    }
}

impl Cache {
    pub async fn check_rate_limit(
        &self,
        name: &str,
        bucket: RateLimitBucket,
    ) -> Result<RateLimitDecision> {
        let key = self.keys().rate_limit(name);
        let mut connection = self.connection().await?;
        let current: usize = redis::cmd("INCR")
            .arg(&key)
            .query_async(&mut connection)
            .await
            .context("failed to increment Valkey rate-limit bucket")?;

        if current == 1 {
            let _: bool = redis::cmd("EXPIRE")
                .arg(&key)
                .arg(duration_secs(bucket.window))
                .query_async(&mut connection)
                .await
                .context("failed to expire Valkey rate-limit bucket")?;
        }

        let allowed = current <= bucket.capacity;
        let remaining = bucket.capacity.saturating_sub(current);
        Ok(RateLimitDecision {
            allowed,
            capacity: bucket.capacity,
            current,
            remaining,
        })
    }
}

fn duration_secs(duration: Duration) -> u64 {
    duration.as_secs().max(1)
}
