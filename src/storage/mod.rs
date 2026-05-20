use anyhow::{Context, Result};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

use crate::config::DatabaseConfig;

pub mod alerts;
pub mod backtests;
pub mod balances;
pub mod fills;
pub mod orders;
pub mod positions;
pub mod risk_events;
pub mod signals;

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub async fn connect_optional(config: &DatabaseConfig) -> Result<Option<Self>> {
        if !config.enabled {
            return Ok(None);
        }

        let url = std::env::var(&config.url_env)
            .with_context(|| format!("{} must be set when database is enabled", config.url_env))?;
        let pool = PgPoolOptions::new()
            .min_connections(config.min_connections)
            .max_connections(config.max_connections)
            .acquire_timeout(Duration::from_secs(config.connect_timeout_secs))
            .connect(&url)
            .await
            .context("failed to connect to Postgres")?;

        Ok(Some(Self { pool }))
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./src/storage/migrations")
            .run(&self.pool)
            .await
            .context("failed to run Postgres migrations")
    }
}

pub async fn migrate_from_config(config: &DatabaseConfig) -> Result<()> {
    let Some(db) = Db::connect_optional(config).await? else {
        tracing::info!("database disabled; skipping migrations");
        return Ok(());
    };
    db.migrate().await
}
