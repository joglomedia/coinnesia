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
pub mod repository;
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

        let db = Self { pool };
        if config.migrate_on_start {
            db.migrate().await?;
        }

        Ok(Some(db))
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

#[cfg(test)]
mod tests {
    use crate::config::{AppConfig, DatabaseConfig};

    use super::Db;

    #[tokio::test]
    async fn disabled_database_does_not_require_url() {
        let config = DatabaseConfig {
            enabled: false,
            url_env: "COINNESIA_TEST_DATABASE_URL_MISSING".to_owned(),
            max_connections: 1,
            min_connections: 0,
            connect_timeout_secs: 1,
            migrate_on_start: false,
        };

        let db = Db::connect_optional(&config)
            .await
            .expect("disabled database should not connect");
        assert!(db.is_none());
    }

    #[test]
    fn default_database_config_is_disabled_safe() {
        let config = AppConfig::from_default_toml().expect("default config parses");
        assert!(!config.database.enabled);
        assert!(!config.database.url_env.trim().is_empty());
    }

    #[test]
    fn migration_catalog_contains_phase_0_4_foundation() {
        let migration_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/storage/migrations");
        let entries = std::fs::read_dir(&migration_dir).expect("migration dir readable");
        let has_phase_0_4 = entries.filter_map(Result::ok).any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains("phase_0_4_foundation")
        });

        assert!(has_phase_0_4);
    }
}
