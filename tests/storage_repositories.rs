use chrono::Utc;
use coinnesia::{
    config::DatabaseConfig,
    storage::{
        orders::OrderRecord, positions::PositionRecord, risk_events::RiskEventRecord,
        signals::SignalRecord, Db,
    },
};
use rust_decimal::Decimal;
use uuid::Uuid;

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
async fn repositories_insert_and_read_core_records_when_database_url_is_set() {
    let Some(config) = test_database_config() else {
        return;
    };

    let db = Db::connect_optional(&config)
        .await
        .expect("test database connects")
        .expect("database enabled");
    let now = Utc::now();

    let signal = SignalRecord::new(
        Uuid::new_v4(),
        "BTCUSDT",
        "WAIT",
        "wait",
        Decimal::new(67, 0),
        Decimal::new(12, 0),
        "repository_test",
        now,
    )
    .with_timeframe("1h")
    .with_asset_class("btc");
    db.signals().append(&signal).await.expect("signal inserts");
    let stored_signal = db.signals().get(signal.id).await.expect("signal reads");
    assert_eq!(stored_signal.symbol, "BTCUSDT");
    assert_eq!(stored_signal.state, "WAIT");

    let order = OrderRecord::new(
        Uuid::new_v4(),
        format!("test-{}", Uuid::new_v4()),
        "BTCUSDT",
        "buy",
        "limit",
        "open",
        Decimal::new(1, 2),
        "paper",
        now,
        now,
    );
    db.orders().append(&order).await.expect("order inserts");
    let stored_order = db.orders().get(order.id).await.expect("order reads");
    assert_eq!(stored_order.symbol, "BTCUSDT");
    assert_eq!(stored_order.status, "open");

    let position = PositionRecord {
        id: Uuid::new_v4(),
        symbol: "BTCUSDT".to_owned(),
        asset_class: "btc".to_owned(),
        side: "long".to_owned(),
        status: "open".to_owned(),
        quantity: Decimal::new(1, 2),
        average_entry: Decimal::new(100000, 0),
        realized_pnl: Decimal::ZERO,
        unrealized_pnl: Decimal::ZERO,
        opened_at: now,
        closed_at: None,
        updated_at: now,
        metadata: serde_json::json!({}),
    };
    db.positions()
        .append(&position)
        .await
        .expect("position inserts");
    let stored_position = db
        .positions()
        .get(position.id)
        .await
        .expect("position reads");
    assert_eq!(stored_position.symbol, "BTCUSDT");
    assert_eq!(stored_position.status, "open");

    let risk_event = RiskEventRecord {
        id: Uuid::new_v4(),
        event_type: "risk_decision".to_owned(),
        severity: "info".to_owned(),
        symbol: Some("BTCUSDT".to_owned()),
        approved: Some(true),
        reason: "approved".to_owned(),
        payload: serde_json::json!({ "source": "repository_test" }),
        occurred_at: now,
    };
    db.risk_events()
        .append(&risk_event)
        .await
        .expect("risk event inserts");
    let stored_risk_event = db
        .risk_events()
        .get(risk_event.id)
        .await
        .expect("risk event reads");
    assert_eq!(stored_risk_event.reason, "approved");

    let kill_switch = db
        .risk_events()
        .kill_switch()
        .await
        .expect("kill switch reads");
    assert!(!kill_switch.triggered);
}
