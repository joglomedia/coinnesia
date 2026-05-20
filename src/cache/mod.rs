use anyhow::{Context, Result};
use redis::AsyncCommands;
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
    prefix: String,
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
            prefix: config.key_prefix.clone(),
        }))
    }

    pub fn key(&self, parts: &[&str]) -> String {
        keys::build_key(&self.prefix, parts)
    }

    pub async fn set_ttl(&self, key: &str, value: &str, ttl: Duration) -> Result<()> {
        let mut connection = self
            .client
            .get_multiplexed_async_connection()
            .await
            .context("failed to connect to Valkey")?;
        let _: () = connection
            .set_ex(key, value, ttl.as_secs())
            .await
            .context("failed to set Valkey key")?;
        Ok(())
    }
}
