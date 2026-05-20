use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

use crate::config::CacheConfig;

pub mod keys;
pub mod locks;
pub mod pubsub;
pub mod rate_limit;
pub mod snapshots;

#[derive(Clone)]
pub struct Cache {
    client: redis::Client,
    keys: keys::KeyBuilder,
    default_ttl: Duration,
}

impl Cache {
    pub async fn connect_optional(config: &CacheConfig) -> Result<Option<Self>> {
        if !config.enabled {
            return Ok(None);
        }

        let url = std::env::var(&config.url_env)
            .with_context(|| format!("{} must be set when cache is enabled", config.url_env))?;
        let client = redis::Client::open(url).context("invalid Valkey URL")?;
        let mut connection = client
            .get_multiplexed_async_connection()
            .await
            .context("failed to connect to Valkey")?;
        let _: String = redis::cmd("PING")
            .query_async(&mut connection)
            .await
            .context("failed to ping Valkey")?;

        Ok(Some(Self {
            client,
            keys: keys::KeyBuilder::new(config.key_prefix.clone()),
            default_ttl: Duration::from_secs(config.ttl_seconds),
        }))
    }

    pub fn client(&self) -> &redis::Client {
        &self.client
    }

    pub fn keys(&self) -> &keys::KeyBuilder {
        &self.keys
    }

    pub fn default_ttl(&self) -> Duration {
        self.default_ttl
    }

    pub fn key(&self, parts: &[&str]) -> String {
        self.keys.build(parts)
    }

    pub(crate) async fn connection(&self) -> Result<redis::aio::MultiplexedConnection> {
        self.client
            .get_multiplexed_async_connection()
            .await
            .context("failed to connect to Valkey")
    }

    pub async fn set_ttl(&self, key: &str, value: &str, ttl: Duration) -> Result<()> {
        let mut connection = self.connection().await?;
        let _: () = connection
            .set_ex(key, value, duration_secs(ttl))
            .await
            .context("failed to set Valkey key")?;
        Ok(())
    }

    pub async fn set_default_ttl(&self, key: &str, value: &str) -> Result<()> {
        self.set_ttl(key, value, self.default_ttl).await
    }

    pub async fn get_string(&self, key: &str) -> Result<Option<String>> {
        let mut connection = self.connection().await?;
        connection
            .get(key)
            .await
            .context("failed to get Valkey key")
    }

    pub async fn set_json<T>(&self, key: &str, value: &T, ttl: Duration) -> Result<()>
    where
        T: Serialize + ?Sized,
    {
        let value = serde_json::to_string(value).context("failed to serialize cache value")?;
        self.set_ttl(key, &value, ttl).await
    }

    pub async fn get_json<T>(&self, key: &str) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        let Some(value) = self.get_string(key).await? else {
            return Ok(None);
        };
        serde_json::from_str(&value)
            .map(Some)
            .context("failed to deserialize cache value")
    }

    pub async fn mark_dedupe(&self, scope: &str, id: &str, ttl: Duration) -> Result<bool> {
        let key = self.keys.dedupe(scope, id);
        let mut connection = self.connection().await?;
        let inserted: bool = redis::cmd("SET")
            .arg(key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(duration_secs(ttl))
            .query_async(&mut connection)
            .await
            .context("failed to set Valkey dedupe key")?;
        Ok(inserted)
    }

    pub async fn heartbeat(&self, component: &str, ttl: Duration) -> Result<DateTime<Utc>> {
        let now = Utc::now();
        let key = self.keys.heartbeat(component);
        self.set_ttl(&key, &now.to_rfc3339(), ttl).await?;
        Ok(now)
    }

    pub async fn get_heartbeat(&self, component: &str) -> Result<Option<DateTime<Utc>>> {
        let key = self.keys.heartbeat(component);
        let Some(value) = self.get_string(&key).await? else {
            return Ok(None);
        };
        DateTime::parse_from_rfc3339(&value)
            .map(|value| Some(value.with_timezone(&Utc)))
            .context("failed to parse heartbeat timestamp")
    }
}

fn duration_secs(duration: Duration) -> u64 {
    duration.as_secs().max(1)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::config::CacheConfig;

    use super::Cache;

    #[tokio::test]
    async fn disabled_cache_does_not_require_url() {
        let config = CacheConfig {
            enabled: false,
            url_env: "COINNESIA_TEST_VALKEY_URL_MISSING".to_owned(),
            key_prefix: "coinnesia_test".to_owned(),
            pool_size: 1,
            ttl_seconds: 60,
        };

        let cache = Cache::connect_optional(&config)
            .await
            .expect("disabled cache should not connect");
        assert!(cache.is_none());
    }

    #[test]
    fn duration_secs_never_returns_zero() {
        assert_eq!(super::duration_secs(Duration::from_millis(1)), 1);
    }
}
