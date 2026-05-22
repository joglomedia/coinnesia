use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::time;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    alerts::{telegram::TelegramAlertSink, AlertSink},
    app::AppState,
    storage::{alerts::AlertDeliveryRecord, alerts::AlertJobRecord, Db},
};

#[derive(Clone)]
pub struct AlertWorker {
    state: AppState,
    sink: Option<Arc<dyn AlertSink>>,
}

impl AlertWorker {
    pub fn from_state(state: AppState) -> Result<Self> {
        let sink = if state.config.alerts.enabled && state.config.alerts.telegram.enabled {
            TelegramAlertSink::from_config(&state.config.alerts.telegram)?
                .map(|sink| Arc::new(sink) as Arc<dyn AlertSink>)
        } else {
            None
        };
        Ok(Self { state, sink })
    }

    pub fn with_sink(state: AppState, sink: Arc<dyn AlertSink>) -> Self {
        Self {
            state,
            sink: Some(sink),
        }
    }

    pub async fn run_until_cancelled(&self, shutdown: CancellationToken) -> Result<()> {
        self.state.health.heartbeat("alert");
        info!(worker = "alert", "worker started");
        let mut interval = time::interval(Duration::from_secs(
            self.state.config.alerts.poll_interval_secs.max(1),
        ));

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    self.state.health.set_component("alert", false);
                    info!(worker = "alert", "worker stopped");
                    return Ok(());
                }
                _ = interval.tick() => {
                    if let Err(error) = self.process_once().await {
                        error!(?error, worker = "alert", "alert worker cycle failed");
                    }
                    self.state.health.heartbeat("alert");
                }
            }
        }
    }

    pub async fn process_once(&self) -> Result<usize> {
        if !self.state.config.alerts.enabled || self.sink.is_none() {
            return Ok(0);
        }

        let Some(db) = &self.state.db else {
            return Ok(0);
        };

        let jobs = db
            .alerts()
            .claim_pending_jobs(self.state.config.alerts.batch_size.max(1))
            .await?;
        let mut processed = 0;

        for job in jobs {
            processed += 1;
            if let Err(error) = self.process_job(db, job).await {
                error!(?error, worker = "alert", "failed to process alert job");
            }
        }

        Ok(processed)
    }

    async fn process_job(&self, db: &Db, job: AlertJobRecord) -> Result<()> {
        if self.is_duplicate(&job).await? {
            db.alerts().mark_deduplicated(job.id).await?;
            return Ok(());
        }

        let message = job
            .payload
            .get("message")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .context("alert job payload missing message")?;

        self.state.metrics.inc_telegram_delivery_attempt();
        let attempted_at = Utc::now();
        let result = match &self.sink {
            Some(sink) => sink.send(&message).await,
            None => Ok(()),
        };

        match result {
            Ok(()) => {
                db.alerts()
                    .append_delivery(&AlertDeliveryRecord::new(
                        Uuid::new_v4(),
                        job.id,
                        job.channel.clone(),
                        true,
                        None,
                        None,
                        attempted_at,
                    ))
                    .await?;
                db.alerts().mark_delivered(job.id, Utc::now()).await?;
                Ok(())
            }
            Err(error) => {
                db.alerts()
                    .append_delivery(&AlertDeliveryRecord::new(
                        Uuid::new_v4(),
                        job.id,
                        job.channel.clone(),
                        false,
                        None,
                        Some(error.to_string()),
                        attempted_at,
                    ))
                    .await?;
                db.alerts().mark_failed(job.id).await?;
                Ok(())
            }
        }
    }

    async fn is_duplicate(&self, job: &AlertJobRecord) -> Result<bool> {
        let Some(cache) = &self.state.cache else {
            return Ok(false);
        };
        let payload_dedupe_key = job
            .payload
            .get("dedupe_key")
            .and_then(|value| value.as_str());
        let Some(dedupe_key) = payload_dedupe_key.or(job.dedupe_key.as_deref()) else {
            return Ok(false);
        };

        let inserted = cache
            .mark_dedupe(
                "telegram",
                dedupe_key,
                Duration::from_secs(self.state.config.alerts.dedupe_ttl_secs.max(1)),
            )
            .await?;
        Ok(!inserted)
    }
}
