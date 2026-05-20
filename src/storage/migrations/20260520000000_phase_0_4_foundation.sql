CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS symbols (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol TEXT NOT NULL UNIQUE,
    asset_class TEXT NOT NULL,
    exchange TEXT NOT NULL,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    timeframes TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS signal_evaluations (
    id UUID PRIMARY KEY,
    symbol TEXT NOT NULL,
    timeframe TEXT,
    asset_class TEXT,
    state TEXT NOT NULL,
    direction TEXT NOT NULL,
    confidence NUMERIC(10, 4) NOT NULL,
    directional_gap NUMERIC(10, 4) NOT NULL DEFAULT 0,
    reason TEXT NOT NULL,
    entry_plan JSONB,
    indicators JSONB NOT NULL DEFAULT '{}'::JSONB,
    evaluated_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_signal_evaluations_symbol_time
    ON signal_evaluations (symbol, evaluated_at DESC);

CREATE INDEX IF NOT EXISTS idx_signal_evaluations_state_time
    ON signal_evaluations (state, evaluated_at DESC);

CREATE TABLE IF NOT EXISTS alert_jobs (
    id UUID PRIMARY KEY,
    signal_id UUID REFERENCES signal_evaluations(id) ON DELETE SET NULL,
    channel TEXT NOT NULL,
    status TEXT NOT NULL,
    payload JSONB NOT NULL,
    dedupe_key TEXT,
    scheduled_at TIMESTAMPTZ NOT NULL,
    delivered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_alert_jobs_dedupe_key
    ON alert_jobs (dedupe_key)
    WHERE dedupe_key IS NOT NULL;

CREATE TABLE IF NOT EXISTS alert_deliveries (
    id UUID PRIMARY KEY,
    alert_job_id UUID NOT NULL REFERENCES alert_jobs(id) ON DELETE CASCADE,
    channel TEXT NOT NULL,
    success BOOLEAN NOT NULL,
    provider_message_id TEXT,
    error TEXT,
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_alert_deliveries_job_time
    ON alert_deliveries (alert_job_id, attempted_at DESC);

CREATE TABLE IF NOT EXISTS orders (
    id UUID PRIMARY KEY,
    client_order_id TEXT NOT NULL UNIQUE,
    exchange_order_id TEXT,
    symbol TEXT NOT NULL,
    side TEXT NOT NULL,
    order_type TEXT NOT NULL,
    status TEXT NOT NULL,
    quantity NUMERIC(38, 18) NOT NULL,
    price NUMERIC(38, 18),
    stop_price NUMERIC(38, 18),
    mode TEXT NOT NULL,
    requested_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    raw JSONB NOT NULL DEFAULT '{}'::JSONB
);

CREATE INDEX IF NOT EXISTS idx_orders_symbol_status
    ON orders (symbol, status);

CREATE INDEX IF NOT EXISTS idx_orders_requested_at
    ON orders (requested_at DESC);

CREATE TABLE IF NOT EXISTS order_events (
    id UUID PRIMARY KEY,
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    status TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::JSONB,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_order_events_order_time
    ON order_events (order_id, occurred_at DESC);

CREATE TABLE IF NOT EXISTS fills (
    id UUID PRIMARY KEY,
    order_id UUID REFERENCES orders(id) ON DELETE SET NULL,
    exchange_fill_id TEXT,
    symbol TEXT NOT NULL,
    side TEXT NOT NULL,
    quantity NUMERIC(38, 18) NOT NULL,
    price NUMERIC(38, 18) NOT NULL,
    fee NUMERIC(38, 18) NOT NULL DEFAULT 0,
    fee_asset TEXT,
    filled_at TIMESTAMPTZ NOT NULL,
    raw JSONB NOT NULL DEFAULT '{}'::JSONB
);

CREATE INDEX IF NOT EXISTS idx_fills_symbol_time
    ON fills (symbol, filled_at DESC);

CREATE TABLE IF NOT EXISTS positions (
    id UUID PRIMARY KEY,
    symbol TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    side TEXT NOT NULL,
    status TEXT NOT NULL,
    quantity NUMERIC(38, 18) NOT NULL,
    average_entry NUMERIC(38, 18) NOT NULL,
    realized_pnl NUMERIC(38, 18) NOT NULL DEFAULT 0,
    unrealized_pnl NUMERIC(38, 18) NOT NULL DEFAULT 0,
    opened_at TIMESTAMPTZ NOT NULL,
    closed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB
);

CREATE INDEX IF NOT EXISTS idx_positions_symbol_status
    ON positions (symbol, status);

CREATE TABLE IF NOT EXISTS balances (
    id UUID PRIMARY KEY,
    asset TEXT NOT NULL,
    available NUMERIC(38, 18) NOT NULL,
    locked NUMERIC(38, 18) NOT NULL,
    source TEXT NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_balances_asset_time
    ON balances (asset, captured_at DESC);

CREATE TABLE IF NOT EXISTS portfolio_snapshots (
    id UUID PRIMARY KEY,
    total_equity NUMERIC(38, 18) NOT NULL,
    reserved_capital NUMERIC(38, 18) NOT NULL,
    exposure JSONB NOT NULL DEFAULT '{}'::JSONB,
    captured_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS risk_events (
    id UUID PRIMARY KEY,
    event_type TEXT NOT NULL,
    severity TEXT NOT NULL,
    symbol TEXT,
    approved BOOLEAN,
    reason TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::JSONB,
    occurred_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_risk_events_type_time
    ON risk_events (event_type, occurred_at DESC);

CREATE TABLE IF NOT EXISTS kill_switch_state (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE,
    triggered BOOLEAN NOT NULL,
    reason TEXT,
    manual_restart_required BOOLEAN NOT NULL,
    triggered_at TIMESTAMPTZ,
    reset_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT kill_switch_state_singleton CHECK (id)
);

INSERT INTO kill_switch_state (id, triggered, reason, manual_restart_required)
VALUES (TRUE, FALSE, NULL, TRUE)
ON CONFLICT (id) DO NOTHING;

CREATE TABLE IF NOT EXISTS backtest_runs (
    id UUID PRIMARY KEY,
    name TEXT,
    status TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ,
    config JSONB NOT NULL,
    metrics JSONB NOT NULL DEFAULT '{}'::JSONB,
    report_path TEXT
);

CREATE INDEX IF NOT EXISTS idx_backtest_runs_started_at
    ON backtest_runs (started_at DESC);

CREATE TABLE IF NOT EXISTS audit_events (
    id UUID PRIMARY KEY,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::JSONB,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_events_resource_time
    ON audit_events (resource, occurred_at DESC);
