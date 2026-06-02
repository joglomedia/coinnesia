use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{query, query_as};
use uuid::Uuid;

use super::{
    alerts::{AlertDeliveryRecord, AlertJobRecord},
    backtests::BacktestRunRecord,
    fills::FillRecord,
    orders::OrderRecord,
    positions::PositionRecord,
    risk_events::{KillSwitchRecord, RiskEventRecord},
    signals::SignalRecord,
    Db,
};

#[derive(Clone)]
pub struct SignalRepository {
    db: Db,
}

#[derive(Clone)]
pub struct OrderRepository {
    db: Db,
}

#[derive(Clone)]
pub struct PositionRepository {
    db: Db,
}

#[derive(Clone)]
pub struct RiskEventRepository {
    db: Db,
}

#[derive(Clone)]
pub struct AlertRepository {
    db: Db,
}

impl Db {
    pub fn signals(&self) -> SignalRepository {
        SignalRepository { db: self.clone() }
    }

    pub fn orders(&self) -> OrderRepository {
        OrderRepository { db: self.clone() }
    }

    pub fn positions(&self) -> PositionRepository {
        PositionRepository { db: self.clone() }
    }

    pub fn risk_events(&self) -> RiskEventRepository {
        RiskEventRepository { db: self.clone() }
    }

    pub fn alerts(&self) -> AlertRepository {
        AlertRepository { db: self.clone() }
    }

    pub async fn insert_signal(&self, signal: &SignalRecord) -> Result<()> {
        self.signals().append(signal).await
    }

    pub async fn fetch_signal(&self, id: Uuid) -> Result<SignalRecord> {
        self.signals().get(id).await
    }

    pub async fn insert_order(&self, order: &OrderRecord) -> Result<()> {
        self.orders().append(order).await
    }

    pub async fn fetch_order(&self, id: Uuid) -> Result<OrderRecord> {
        self.orders().get(id).await
    }

    pub async fn insert_position(&self, position: &PositionRecord) -> Result<()> {
        self.positions().append(position).await
    }

    pub async fn fetch_position(&self, id: Uuid) -> Result<PositionRecord> {
        self.positions().get(id).await
    }

    pub async fn insert_risk_event(&self, event: &RiskEventRecord) -> Result<()> {
        self.risk_events().append(event).await
    }

    pub async fn fetch_risk_event(&self, id: Uuid) -> Result<RiskEventRecord> {
        self.risk_events().get(id).await
    }

    pub async fn fetch_kill_switch(&self) -> Result<KillSwitchRecord> {
        self.risk_events().kill_switch().await
    }

    pub async fn insert_fill(&self, fill: &FillRecord) -> Result<()> {
        query(
            r#"
            INSERT INTO fills (
                id, order_id, exchange_fill_id, symbol, side, quantity, price,
                fee, fee_asset, filled_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            "#,
        )
        .bind(fill.id)
        .bind(fill.order_id)
        .bind(&fill.exchange_fill_id)
        .bind(&fill.symbol)
        .bind(&fill.side)
        .bind(fill.quantity)
        .bind(fill.price)
        .bind(fill.fee)
        .bind(&fill.fee_asset)
        .bind(fill.filled_at)
        .execute(self.pool())
        .await
        .context("failed to insert fill")?;
        Ok(())
    }

    pub async fn fetch_backtest_run(&self, id: Uuid) -> Result<BacktestRunRecord> {
        query_as::<_, BacktestRunRecord>(
            r#"
            SELECT id, name, status, started_at, finished_at
            FROM backtest_runs
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_one(self.pool())
        .await
        .context("failed to fetch backtest run")
    }
}

impl AlertRepository {
    pub async fn append_job(&self, job: &AlertJobRecord) -> Result<()> {
        query(
            r#"
            INSERT INTO alert_jobs (
                id, signal_id, channel, status, payload, dedupe_key,
                scheduled_at, delivered_at, created_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
            ON CONFLICT (dedupe_key) WHERE dedupe_key IS NOT NULL DO NOTHING
            "#,
        )
        .bind(job.id)
        .bind(job.signal_id)
        .bind(&job.channel)
        .bind(&job.status)
        .bind(&job.payload)
        .bind(&job.dedupe_key)
        .bind(job.scheduled_at)
        .bind(job.delivered_at)
        .bind(job.created_at)
        .execute(self.db.pool())
        .await
        .context("failed to insert alert job")?;
        Ok(())
    }

    pub async fn get_job(&self, id: Uuid) -> Result<AlertJobRecord> {
        query_as::<_, AlertJobRecord>(
            r#"
            SELECT
                id, signal_id, channel, status, payload, dedupe_key,
                scheduled_at, delivered_at, created_at
            FROM alert_jobs
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_one(self.db.pool())
        .await
        .context("failed to fetch alert job")
    }

    pub async fn claim_pending_jobs(&self, limit: usize) -> Result<Vec<AlertJobRecord>> {
        query_as::<_, AlertJobRecord>(
            r#"
            WITH selected AS (
                SELECT id
                FROM alert_jobs
                WHERE status = 'pending'
                  AND scheduled_at <= NOW()
                ORDER BY scheduled_at ASC
                LIMIT $1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE alert_jobs AS job
            SET status = 'processing'
            FROM selected
            WHERE job.id = selected.id
            RETURNING
                job.id, job.signal_id, job.channel, job.status, job.payload,
                job.dedupe_key, job.scheduled_at, job.delivered_at, job.created_at
            "#,
        )
        .bind(limit as i64)
        .fetch_all(self.db.pool())
        .await
        .context("failed to claim pending alert jobs")
    }

    pub async fn mark_delivered(&self, id: Uuid, delivered_at: DateTime<Utc>) -> Result<()> {
        query(
            r#"
            UPDATE alert_jobs
            SET status = 'delivered', delivered_at = $2
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(delivered_at)
        .execute(self.db.pool())
        .await
        .context("failed to mark alert job delivered")?;
        Ok(())
    }

    pub async fn mark_failed(&self, id: Uuid) -> Result<()> {
        query(
            r#"
            UPDATE alert_jobs
            SET status = 'failed'
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(self.db.pool())
        .await
        .context("failed to mark alert job failed")?;
        Ok(())
    }

    pub async fn mark_deduplicated(&self, id: Uuid) -> Result<()> {
        query(
            r#"
            UPDATE alert_jobs
            SET status = 'deduplicated'
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(self.db.pool())
        .await
        .context("failed to mark alert job deduplicated")?;
        Ok(())
    }

    pub async fn append_delivery(&self, delivery: &AlertDeliveryRecord) -> Result<()> {
        query(
            r#"
            INSERT INTO alert_deliveries (
                id, alert_job_id, channel, success, provider_message_id,
                error, attempted_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7)
            "#,
        )
        .bind(delivery.id)
        .bind(delivery.alert_job_id)
        .bind(&delivery.channel)
        .bind(delivery.success)
        .bind(&delivery.provider_message_id)
        .bind(&delivery.error)
        .bind(delivery.attempted_at)
        .execute(self.db.pool())
        .await
        .context("failed to insert alert delivery attempt")?;
        Ok(())
    }

    pub async fn deliveries_for_job(&self, alert_job_id: Uuid) -> Result<Vec<AlertDeliveryRecord>> {
        query_as::<_, AlertDeliveryRecord>(
            r#"
            SELECT
                id, alert_job_id, channel, success, provider_message_id,
                error, attempted_at
            FROM alert_deliveries
            WHERE alert_job_id = $1
            ORDER BY attempted_at ASC
            "#,
        )
        .bind(alert_job_id)
        .fetch_all(self.db.pool())
        .await
        .context("failed to fetch alert delivery attempts")
    }
}

impl SignalRepository {
    pub async fn append(&self, signal: &SignalRecord) -> Result<()> {
        query(
            r#"
            INSERT INTO signal_evaluations (
                id, symbol, timeframe, asset_class, state, direction,
                confidence, directional_gap, reason, entry_plan, indicators,
                evaluated_at, panel_report
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
            "#,
        )
        .bind(signal.id)
        .bind(&signal.symbol)
        .bind(&signal.timeframe)
        .bind(&signal.asset_class)
        .bind(&signal.state)
        .bind(&signal.direction)
        .bind(signal.confidence)
        .bind(signal.directional_gap)
        .bind(&signal.reason)
        .bind(&signal.entry_plan)
        .bind(&signal.indicators)
        .bind(signal.evaluated_at)
        .bind(&signal.panel_report)
        .execute(self.db.pool())
        .await
        .context("failed to insert signal evaluation")?;
        Ok(())
    }

    pub async fn get(&self, id: Uuid) -> Result<SignalRecord> {
        query_as::<_, SignalRecord>(
            r#"
            SELECT
                id, symbol, timeframe, asset_class, state, direction,
                confidence, directional_gap, reason, entry_plan, indicators,
                evaluated_at, panel_report
            FROM signal_evaluations
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_one(self.db.pool())
        .await
        .context("failed to fetch signal evaluation")
    }

    pub async fn get_by_cycle_symbol(&self, cycle_id: Uuid, symbol: &str) -> Result<SignalRecord> {
        query_as::<_, SignalRecord>(
            r#"
            SELECT
                id, symbol, timeframe, asset_class, state, direction,
                confidence, directional_gap, reason, entry_plan, indicators,
                evaluated_at, panel_report
            FROM signal_evaluations
            WHERE indicators->>'cycle_id' = $1
              AND symbol = $2
            ORDER BY evaluated_at DESC
            LIMIT 1
            "#,
        )
        .bind(cycle_id.to_string())
        .bind(symbol)
        .fetch_one(self.db.pool())
        .await
        .context("failed to fetch signal evaluation by cycle and symbol")
    }

    /// Return the latest persisted signal for a symbol, regardless of cycle.
    /// Used by the `GET /signals/:symbol` API surface (sub-phase 1.7.11).
    pub async fn latest_for_symbol(&self, symbol: &str) -> Result<Option<SignalRecord>> {
        let record = query_as::<_, SignalRecord>(
            r#"
            SELECT
                id, symbol, timeframe, asset_class, state, direction,
                confidence, directional_gap, reason, entry_plan, indicators,
                evaluated_at, panel_report
            FROM signal_evaluations
            WHERE symbol = $1
            ORDER BY evaluated_at DESC
            LIMIT 1
            "#,
        )
        .bind(symbol)
        .fetch_optional(self.db.pool())
        .await
        .context("failed to fetch latest signal evaluation for symbol")?;
        Ok(record)
    }
}

impl OrderRepository {
    pub async fn append(&self, order: &OrderRecord) -> Result<()> {
        query(
            r#"
            INSERT INTO orders (
                id, client_order_id, exchange_order_id, symbol, side, order_type,
                status, quantity, price, stop_price, mode, requested_at, updated_at, raw
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            "#,
        )
        .bind(order.id)
        .bind(&order.client_order_id)
        .bind(&order.exchange_order_id)
        .bind(&order.symbol)
        .bind(&order.side)
        .bind(&order.order_type)
        .bind(&order.status)
        .bind(order.quantity)
        .bind(order.price)
        .bind(order.stop_price)
        .bind(&order.mode)
        .bind(order.requested_at)
        .bind(order.updated_at)
        .bind(&order.raw)
        .execute(self.db.pool())
        .await
        .context("failed to insert order")?;
        Ok(())
    }

    pub async fn get(&self, id: Uuid) -> Result<OrderRecord> {
        query_as::<_, OrderRecord>(
            r#"
            SELECT
                id, client_order_id, exchange_order_id, symbol, side, order_type,
                status, quantity, price, stop_price, mode, requested_at, updated_at, raw
            FROM orders
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_one(self.db.pool())
        .await
        .context("failed to fetch order")
    }
}

impl PositionRepository {
    pub async fn append(&self, position: &PositionRecord) -> Result<()> {
        query(
            r#"
            INSERT INTO positions (
                id, symbol, asset_class, side, status, quantity, average_entry,
                realized_pnl, unrealized_pnl, opened_at, closed_at, updated_at, metadata
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
            "#,
        )
        .bind(position.id)
        .bind(&position.symbol)
        .bind(&position.asset_class)
        .bind(&position.side)
        .bind(&position.status)
        .bind(position.quantity)
        .bind(position.average_entry)
        .bind(position.realized_pnl)
        .bind(position.unrealized_pnl)
        .bind(position.opened_at)
        .bind(position.closed_at)
        .bind(position.updated_at)
        .bind(&position.metadata)
        .execute(self.db.pool())
        .await
        .context("failed to insert position")?;
        Ok(())
    }

    pub async fn get(&self, id: Uuid) -> Result<PositionRecord> {
        query_as::<_, PositionRecord>(
            r#"
            SELECT
                id, symbol, asset_class, side, status, quantity, average_entry,
                realized_pnl, unrealized_pnl, opened_at, closed_at, updated_at, metadata
            FROM positions
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_one(self.db.pool())
        .await
        .context("failed to fetch position")
    }
}

impl RiskEventRepository {
    pub async fn append(&self, event: &RiskEventRecord) -> Result<()> {
        query(
            r#"
            INSERT INTO risk_events (
                id, event_type, severity, symbol, approved, reason, payload, occurred_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            "#,
        )
        .bind(event.id)
        .bind(&event.event_type)
        .bind(&event.severity)
        .bind(&event.symbol)
        .bind(event.approved)
        .bind(&event.reason)
        .bind(&event.payload)
        .bind(event.occurred_at)
        .execute(self.db.pool())
        .await
        .context("failed to insert risk event")?;
        Ok(())
    }

    pub async fn get(&self, id: Uuid) -> Result<RiskEventRecord> {
        query_as::<_, RiskEventRecord>(
            r#"
            SELECT
                id, event_type, severity, symbol, approved, reason, payload, occurred_at
            FROM risk_events
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_one(self.db.pool())
        .await
        .context("failed to fetch risk event")
    }

    pub async fn kill_switch(&self) -> Result<KillSwitchRecord> {
        query_as::<_, KillSwitchRecord>(
            r#"
            SELECT triggered, reason, manual_restart_required, triggered_at, reset_at, updated_at
            FROM kill_switch_state
            WHERE id = TRUE
            "#,
        )
        .fetch_one(self.db.pool())
        .await
        .context("failed to fetch kill switch state")
    }
}

impl SignalRecord {
    pub fn with_timeframe(mut self, timeframe: impl Into<String>) -> Self {
        self.timeframe = Some(timeframe.into());
        self
    }

    pub fn with_asset_class(mut self, asset_class: impl Into<String>) -> Self {
        self.asset_class = Some(asset_class.into());
        self
    }
}

impl OrderRecord {
    pub fn new(
        id: Uuid,
        client_order_id: impl Into<String>,
        symbol: impl Into<String>,
        side: impl Into<String>,
        order_type: impl Into<String>,
        status: impl Into<String>,
        quantity: Decimal,
        mode: impl Into<String>,
        requested_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            client_order_id: client_order_id.into(),
            exchange_order_id: None,
            symbol: symbol.into(),
            side: side.into(),
            order_type: order_type.into(),
            status: status.into(),
            quantity,
            price: None,
            stop_price: None,
            mode: mode.into(),
            requested_at,
            updated_at,
            raw: serde_json::json!({}),
        }
    }
}
