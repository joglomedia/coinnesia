use anyhow::{Context, Result};
use serde::Serialize;

use super::Cache;

#[derive(Debug, Clone)]
pub struct PubSubTopic {
    pub name: String,
}

impl PubSubTopic {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Cache {
    pub async fn publish(&self, topic: &str, payload: &str) -> Result<usize> {
        let topic = self.keys().topic(topic);
        let mut connection = self.connection().await?;
        let subscribers = redis::cmd("PUBLISH")
            .arg(topic)
            .arg(payload)
            .query_async(&mut connection)
            .await
            .context("failed to publish Valkey message")?;
        Ok(subscribers)
    }

    pub async fn publish_json<T>(&self, topic: &str, payload: &T) -> Result<usize>
    where
        T: Serialize + ?Sized,
    {
        let payload =
            serde_json::to_string(payload).context("failed to serialize pub/sub event")?;
        self.publish(topic, &payload).await
    }

    pub async fn subscribe(&self, topic: &str) -> Result<redis::aio::PubSub> {
        let mut pubsub = self
            .client()
            .get_async_pubsub()
            .await
            .context("failed to open Valkey pub/sub connection")?;
        pubsub
            .subscribe(self.keys().topic(topic))
            .await
            .context("failed to subscribe to Valkey topic")?;
        Ok(pubsub)
    }
}
