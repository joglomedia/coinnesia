# Architecture Audit

## Verdict

The current codebase is a solid domain scaffold, but it is not yet a production architecture for a 24/7 trading bot and portfolio manager.

It has the right module names and some core primitives, but it still lacks:

- a long-running service runtime
- a web API layer
- persistent storage
- cache/pub-sub infrastructure
- real exchange execution
- state reconciliation
- operational observability and recovery

## What Is Good Already

- Clear module separation for indicators, strategy, assets, exchange, risk, portfolio, trading, backtest, alerts, and scanner.
- Config-driven shape with TOML.
- Deterministic indicator foundations for EMA, ATR, and RSI.
- Exchange abstraction exists, which prevents hard-coding vendor SDKs into core logic.
- ATR-based entry planning is present.
- `tracing` is already used, which is the right logging base for a service.

## Gaps To Fix

### 1. No 24/7 service kernel

`main.rs` is still a thin CLI entry point. For a bot that runs continuously, the project needs a service kernel that owns:

- runtime startup and shutdown
- background workers
- signal handling
- health reporting
- reconnect/retry supervision
- periodic scan scheduling

Current state: one-shot commands and placeholders.

Required change:

- introduce a top-level application layer, e.g. `app/` or `runtime/`
- add graceful shutdown with `tokio::signal`
- run scanner, trading, alerting, and API server under one supervisor

### 2. No web API

The project has no HTTP API. Since the goal includes CLI and web API, add Axum.

Required change:

- use `axum` for the server
- expose `/health`, `/ready`, `/metrics`, `/config`, `/signals`, `/positions`, `/orders`, `/portfolio`, `/risk`, `/scan`, `/trade`
- add request auth from day one
- keep API handlers thin; call application services only

Recommended role of the API:

- operational control
- status inspection
- manual rescans
- trading enable/disable
- risk and portfolio visibility
- alert history

### 3. No persistent database

The current code has no durable storage. For a trading bot and portfolio manager, that is a major gap.

Use Postgres for durable state:

- symbols and asset profiles
- app configuration snapshots
- signals
- alerts
- orders
- fills
- positions
- balances
- trades
- portfolio snapshots
- risk events
- backtest runs

Recommended change:

- add Postgres with a pool-based async client
- add migrations
- separate write models from hot-path runtime state
- keep indicator calculation state in memory/cache, not in SQL on every tick

### 4. No cache / fast state layer

Use Valkey as the low-latency cache and coordination layer.

Why Valkey:

- fast symbol/session/scan state
- shared runtime snapshots
- distributed locks
- pub/sub for events
- rate-limit tokens
- cached indicator snapshots

Recommended change:

- use a Redis-compatible client against Valkey
- store ephemeral state in Valkey
- use Postgres for durable records only
- do not query Postgres on the hot scan path

### 5. Scanner is not a real pipeline yet

`scanner::scan_once` currently spawns per-symbol tasks, but it does not fetch data, aggregate indicators, or emit persisted events.

Required change:

- separate ingestion, analysis, and publishing
- use batch fetch where possible
- prefetch proxy symbols once per cycle
- avoid cloning large config state on every task
- push scan results into cache and database

### 6. Trading engine is still a shell

The trading module must become an execution service, not just a type container.

Required change:

- add order state machine
- persist order lifecycle
- reconcile exchange state on startup and periodically
- support paper/live parity
- support partial fills, cancel/replace, and stale order cleanup
- enforce risk approval before any exchange call

### 7. Portfolio and risk need persistent state

Portfolio and risk logic must survive restarts.

Required change:

- persist positions, balances, and exposure snapshots in Postgres
- keep fast lookup and locks in Valkey
- make kill switch state durable
- add rehydration on startup

### 8. Alerting needs a queue

Telegram delivery should not be a direct side effect from the scan path.

Required change:

- emit alert jobs into a queue
- have a worker send Telegram messages asynchronously
- persist alert history
- deduplicate repeated signals

### 9. Observability is incomplete

For a 24/7 server, you need more than logs.

Required change:

- structured tracing already exists, keep it
- add metrics endpoints
- add internal counters for scans, API latency, exchange latency, alert success, order failures, and kill-switch trips
- add uptime and last-success timestamps

### 10. Config and secrets need production handling

Current config is TOML-only with placeholders.

Required change:

- support environment overrides
- keep secrets out of committed config
- separate non-secret app config from credentials
- support config reload or controlled restart semantics

## Recommended Stack

- Runtime: Tokio
- Web API: Axum
- Database: Postgres
- Cache / pub-sub / locks: Valkey
- Exchange abstraction: existing `Exchange` trait
- Logging / tracing: `tracing`
- Metrics: Prometheus-compatible exporter or Axum endpoint

## Recommended Process Model

One binary, multiple modes:

- `serve` - start Axum API plus background workers for 24/7 operation
- `scan` - continuous scanner loop
- `trade` - live/paper trading loop
- `backtest` - offline simulation
- `migrate` - database migrations
- `check-config` - configuration validation

This keeps deployment simple while preserving a single shared engine.

## Hot Path vs Cold Path

### Hot path

Must stay memory-first and low allocation:

- candle ingestion
- indicator computation
- signal scoring
- risk approval
- order routing
- alert emission

### Cold path

Can use database and heavier operations:

- historical queries
- audits
- reports
- backtest runs
- admin APIs
- reconciliation

## Performance Changes Needed

To stay fast, the code should:

- batch market-data requests
- reuse buffers where possible
- avoid unnecessary cloning in scan loops
- keep floats in the indicator path and convert to decimal at execution boundary
- use bounded task concurrency
- cache proxy symbols per cycle
- cache indicator outputs keyed by symbol/timeframe/candle hash

## Priority Order

1. Add Axum server and service runtime.
2. Add Postgres persistence.
3. Add Valkey cache/pub-sub/locks.
4. Implement market-data ingestion and proxy prefetch.
5. Build real strategy scoring and risk approval.
6. Implement Binance live exchange adapter and paper parity.
7. Add alert queue and Telegram worker.
8. Add reconciliation and operational metrics.
9. Complete backtest parity using the same engine.

## Conclusion

The current architecture is directionally correct at the domain level, but it needs infrastructure, durability, and service orchestration before it can be considered a real 24/7 trading platform.

