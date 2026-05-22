use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use coinnesia::{
    alerts::{worker::AlertWorker, AlertSink},
    app::AppState,
    config::DatabaseConfig,
    storage::{alerts::AlertJobRecord, signals::SignalRecord},
};
use rust_decimal::Decimal;
use uuid::Uuid;

#[derive(Debug, Clone)]
struct FailingSink;

#[async_trait]
impl AlertSink for FailingSink {
    async fn send(&self, _message: &str) -> Result<()> {
        anyhow::bail!("telegram unavailable")
    }
}

fn test_database_config() -> Option<DatabaseConfig> {
    std::env::var("DATABASE_URL").ok()?;
    Some(DatabaseConfig {
        enabled: true,
        url_env: "DATABASE_URL".to_owned(),
        max_connections: 2,
        min_connections: 0,
        connect_timeout_secs: 5,
        migrate_on_start: true,
    })
}

#[tokio::test]
async fn alert_worker_records_telegram_failure_without_crashing() {
    let Some(database_config) = test_database_config() else {
        return;
    };

    let mut config = coinnesia::config::AppConfig::from_default_toml().expect("config parses");
    config.database = database_config;
    config.alerts.enabled = true;
    config.alerts.telegram.enabled = true;
    config.alerts.batch_size = 10;

    let state = AppState::bootstrap(config.clone())
        .await
        .expect("state boots");
    let db = state.db.clone().expect("database enabled");

    let signal = SignalRecord::new(
        Uuid::new_v4(),
        format!("TEST{}USDT", Uuid::new_v4().simple()),
        "LONG",
        "long",
        Decimal::new(82, 0),
        Decimal::new(62, 0),
        "alert_worker_test",
        Utc::now(),
    )
    .with_timeframe("1h")
    .with_asset_class("btc");
    db.signals().append(&signal).await.expect("signal inserts");

    let job = AlertJobRecord::pending(
        Uuid::new_v4(),
        Some(signal.id),
        "telegram",
        serde_json::json!({
            "message": "test alert message"
        }),
        Some(format!("alert-worker-test:{}", Uuid::new_v4())),
        Utc::now(),
    );
    db.alerts().append_job(&job).await.expect("job inserts");

    let worker = AlertWorker::with_sink(state, Arc::new(FailingSink));
    let processed = worker.process_once().await.expect("worker does not crash");

    assert_eq!(processed, 1);
    let stored_job = db.alerts().get_job(job.id).await.expect("job reads");
    assert_eq!(stored_job.status, "failed");
    let deliveries = db
        .alerts()
        .deliveries_for_job(job.id)
        .await
        .expect("delivery attempts read");
    assert_eq!(deliveries.len(), 1);
    assert!(!deliveries[0].success);
    assert!(deliveries[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("telegram unavailable"));
}
