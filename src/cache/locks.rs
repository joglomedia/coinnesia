use std::time::Duration;

use anyhow::{Context, Result};
use uuid::Uuid;

use super::Cache;

#[derive(Debug, Clone)]
pub struct LockKey {
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheLock {
    pub key: String,
    pub token: String,
}

impl Cache {
    pub async fn acquire_lock(&self, name: &str, ttl: Duration) -> Result<Option<CacheLock>> {
        let key = self.keys().lock(name);
        let token = Uuid::new_v4().to_string();
        let mut connection = self.connection().await?;
        let acquired: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg(&token)
            .arg("NX")
            .arg("PX")
            .arg(duration_millis(ttl))
            .query_async(&mut connection)
            .await
            .context("failed to acquire Valkey lock")?;

        Ok(acquired.map(|_| CacheLock { key, token }))
    }

    pub async fn release_lock(&self, lock: &CacheLock) -> Result<bool> {
        let mut connection = self.connection().await?;
        let released: i32 = redis::Script::new(
            r#"
            if redis.call("GET", KEYS[1]) == ARGV[1] then
                return redis.call("DEL", KEYS[1])
            end
            return 0
            "#,
        )
        .key(&lock.key)
        .arg(&lock.token)
        .invoke_async(&mut connection)
        .await
        .context("failed to release Valkey lock")?;

        Ok(released == 1)
    }
}

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis())
        .unwrap_or(u64::MAX)
        .max(1)
}
