# Execution Development Plan

This plan turns the development timeline in `docs/requirements.md` into an implementation sequence for the current source skeleton.

## Current Skeleton Audit

The repository is a compileable Rust scaffold with 73 source files. It already has:

- CLI commands for `check-config`, `scan-once`, `scan`, and `backtest`
- typed TOML config including server, database, cache, runtime, exchange, trading, portfolio, risk, and backtest sections
- base domain types for assets, timeframes, and candles
- indicator modules with working EMA, ATR/RMA, RSI, VWAP, volume ratio, MACD scaffold, and placeholder advanced indicators
- strategy result types, confidence type, session classifier, trap decision type, and ATR-based EW/TP/SL
- exchange trait plus paper exchange stub
- placeholder scanner, data providers, trading, portfolio, risk, alerts, and backtest modules

Major missing architecture pieces:

- no `app/` service kernel or worker supervisor
- no `api/` Axum server
- no `storage/` Postgres layer or migrations
- no `cache/` Valkey layer
- no `observability/` health/readiness/metrics layer
- no real market data ingestion
- no real strategy scoring pipeline
- no durable order/position/risk state
- no Telegram queue/worker
- no live Binance implementation
- no event-driven backtester

## Delivery Principles

- Build infrastructure before deep strategy work so scanner/trading outputs have stable persistence and API surfaces.
- Keep the hot path memory-first: scan, indicators, signal scoring, risk approval, and order routing must not block on non-critical Postgres writes.
- Use Postgres for durable state and Valkey for hot state, locks, pub/sub, dedupe, and TTL caches.
- Keep Axum handlers thin and move business logic into application services.
- Preserve the existing `Exchange` trait boundary and shared live/paper/backtest engine rule.
- Ship each phase with tests and a CLI/API smoke path.

## Phase 0 — Production Runtime Foundation

Estimate: 25-35 hours.

Goal: make the project runnable as a 24/7 service shell with API, storage, cache, health, and graceful shutdown. This phase does not need full trading logic.

### 0.1 Dependencies And Module Scaffold

Estimate: 2-3 hours.

Tasks:

- Add dependencies: `axum`, `tower`, `tower-http`, `sqlx` with Postgres/runtime features, Redis-compatible Valkey client (`fred` or `redis`), `uuid`, `tokio-util`, metrics dependency.
- Add modules: `src/app`, `src/api`, `src/storage`, `src/cache`, `src/observability`.
- Add CLI subcommands: `serve`, `trade`, `migrate`.
- Keep existing commands working.

Acceptance:

- `cargo test` passes.
- `cargo run -- check-config` still works.
- `cargo run -- serve` starts a minimal service and exits cleanly on Ctrl-C.

### 0.2 Axum API Skeleton

Estimate: 6 hours.

Tasks:

- Build `api::router(AppState)` with `/health`, `/ready`, `/metrics`, `/config`.
- Add token auth middleware/extractor using `server.auth_token_env`.
- Add DTOs and error mapping.
- Ensure config endpoint returns sanitized config only.

Acceptance:

- Route tests cover health, readiness, config sanitization, auth success/failure.
- Mutating route scaffolds reject unauthenticated requests.

### 0.3 Service Kernel And Supervision

Estimate: 7 hours.

Tasks:

- Implement `app::AppState` holding config, health registry, optional Postgres pool, optional Valkey client, service handles.
- Implement graceful shutdown with cancellation token.
- Add worker supervisor abstraction for scanner, alert, trading, reconciliation workers.
- Implement startup gate: live trading remains disabled until reconciliation passes.

Acceptance:

- `serve` starts Axum and placeholder workers.
- Ctrl-C triggers graceful shutdown within `runtime.shutdown_timeout_secs`.
- Health/readiness reflects worker status.

### 0.4 Postgres Foundation

Estimate: 8 hours.

Tasks:

- Add `storage::Db` pool creation from `DATABASE_URL`.
- Add migration runner.
- Create base migrations for:
  - symbols
  - signal evaluations
  - alert jobs / deliveries
  - orders / order events
  - fills
  - positions
  - balances / portfolio snapshots
  - risk events / kill switch state
  - backtest runs
  - audit events
- Add repository traits/structs for append/read operations.

Acceptance:

- `cargo run -- migrate` applies migrations.
- Repository tests can insert/read signal, order, position, and risk records.

### 0.5 Valkey Foundation

Estimate: 6 hours.

Tasks:

- Add `cache::Cache` client from `VALKEY_URL`.
- Add key builder with configured prefix.
- Implement TTL set/get, dedupe key, heartbeat, lock acquire/release, pub/sub wrapper.
- Add rate-limit bucket helper.

Acceptance:

- Cache tests cover key prefixing, TTL behavior, lock semantics, dedupe, and disabled-cache mode.

### 0.6 Observability

Estimate: 4 hours.

Tasks:

- Add health/readiness model for API, scanner, database, Valkey, exchange, alert worker, trading worker.
- Add basic metrics counters/timers:
  - scan cycles
  - symbols scanned
  - signals generated
  - API request count/latency
  - exchange errors
  - Telegram delivery attempts
  - kill-switch events

Acceptance:

- `/health`, `/ready`, and `/metrics` return meaningful state.
- Worker heartbeat staleness affects readiness.

## Phase 1 — Core Scanner

Estimate: 40-50 hours.

Goal: make `scan-once` and `scan` fetch real data, compute indicators, score signals, cache/persist outputs, and enqueue Telegram alerts.

### 1.1 Market Data Ingestion

Estimate: 5 hours.

Tasks:

- Expand `MarketDataSource` to support batch candles and quotes.
- Implement Binance crypto OHLCV fetch.
- Implement Yahoo Finance fallback for daily/proxy data.
- Add TradingView adapter scaffold if auth details are available.
- Implement proxy prefetch once per cycle for XAUUSD/IHSG/DXY.

Acceptance:

- Integration test can fetch or mock candles for BTCUSDT and proxy symbols.
- Scanner no longer passes empty candles to strategy.

### 1.2 Indicator Completion

Estimate: 22 hours.

Tasks:

- Complete ADX/DMI, MACD parity, VWAP session anchoring, volume engine.
- Implement candle shape, SMC, liquidity sweeps, order blocks, support/resistance, regime classifier.
- Add fixture-backed TradingView/Pine parity tests.

Acceptance:

- Indicator modules have deterministic tests.
- RSI remains RMA-based.
- ATR remains the distance backbone.

### 1.3 Strategy Engine

Estimate: 14 hours.

Tasks:

- Implement six-layer signal evaluation: Trend, Momentum, Volume, Entry Trigger, Anti-Trap, Regime/Session.
- Implement confidence scorer from asset profile weights.
- Implement directional gap checks by timeframe.
- Complete MTF aggregation and session gating.
- Complete trap guard and regime blocking.

Acceptance:

- Strategy tests cover LONG, SHORT, WAIT, FREEZE.
- Trap and shock conditions block tradable signals.
- Same candle fixture produces asset-specific signal differences.

### 1.4 Scanner Pipeline

Estimate: 5-7 hours.

Tasks:

- Split scanner into ingestion, analysis, publishing.
- Bound concurrency using `runtime.max_symbol_tasks`.
- Cache latest scan/signal snapshots in Valkey.
- Persist signal evaluations to Postgres through a bounded worker.
- Enqueue alert jobs for new signals.

Acceptance:

- `scan-once` produces persisted and cached signal evaluations.
- `scan` runs continuously at configured interval.
- Slow persistence or alert worker does not block symbol scanning.

### 1.5 Telegram Alert Worker

Estimate: 3 hours.

Tasks:

- Implement Telegram sender from queued alert jobs.
- Persist delivery attempts.
- Deduplicate repeated signal alerts with Valkey.
- Keep TP3 labelled optional.

Acceptance:

- Alert formatting test verifies TP3 optional.
- Worker handles Telegram failure without crashing scanner.

## Phase 2 — Exchange And Trading

Estimate: 25-30 hours.

Goal: implement exchange execution and paper/live parity behind the `Exchange` trait.

### 2.1 Exchange Trait Expansion

Estimate: 3 hours.

Tasks:

- Expand trait for balances, positions, open orders, order status, symbol info, orderbook/ticker, and exchange metadata.
- Add error types for rate limits, rejected orders, auth failure, network failure, and unsupported operations.

Acceptance:

- Mock exchange tests cover all trait methods.

### 2.2 Binance Adapter

Estimate: 6 hours.

Tasks:

- Implement authenticated Binance REST/WebSocket client behind `Exchange`.
- Support testnet.
- Implement exchange info, filters, precision, balances, order placement, cancel, open orders, fills.

Acceptance:

- Testnet or mock tests verify place/cancel/balance/open-order paths.
- Trading code does not import Binance SDK directly.

### 2.3 Paper Exchange

Estimate: 3 hours.

Tasks:

- Implement realistic paper fills, balances, fees, slippage, partial fills, stop/OCO behavior where possible.
- Persist paper orders/fills to Postgres.

Acceptance:

- Paper and live use the same `Exchange` trait and trading engine.

### 2.4 Trading Engine

Estimate: 15 hours.

Tasks:

- Implement executor, order manager, position tracker, scaling engine.
- Persist order state transitions and fills.
- Handle partial fills, cancel/replace, stale orders.
- Integrate EW plan with scaling percentages.
- Enforce risk approval before every order.

Acceptance:

- Signal -> Risk -> Position Size -> Order -> Exchange flow is covered by integration tests.
- EW1+EW2+EW3+Deep never exceeds planned size.
- Live trading remains blocked when kill switch or reconciliation gate is active.

### 2.5 Reconciliation

Estimate: included in trading/runtime overlap.

Tasks:

- Reconcile exchange balances, positions, orders, fills against Postgres on startup and schedule.
- Detect orphan orders and mismatched positions.
- Expose reconciliation status through `/ready` and `/risk`.

Acceptance:

- Trading enable fails if reconciliation has not completed.

## Phase 3 — Portfolio And Risk

Estimate: 20-25 hours.

Goal: make the bot a portfolio/risk manager, not just an order router.

### 3.1 Portfolio Manager

Estimate: 6 hours.

Tasks:

- Implement allocation by asset class and symbol.
- Track available, locked, unrealized PnL, exposure, and reserved capital.
- Persist portfolio snapshots.
- Expose portfolio API.

Acceptance:

- Allocation and exposure tests pass.
- Portfolio survives restart from Postgres.

### 3.2 Risk Manager

Estimate: 6 hours.

Tasks:

- Implement position sizing methods.
- Enforce max risk per trade, min RR, daily trade limit, cooldown after loss.
- Enforce exposure caps by symbol, asset, total, and correlated groups.
- Persist risk decisions.

Acceptance:

- Risk can approve, reduce, or veto trades with reasons.
- Tests cover risk veto scenarios.

### 3.3 Kill Switch

Estimate: 3 hours.

Tasks:

- Persist kill switch state in Postgres.
- Trigger on drawdown, API errors, manual API call, or critical reconciliation mismatch.
- Require manual reset/restart policy.

Acceptance:

- Kill switch blocks trading after restart.
- API can trigger/reset with audit events.

### 3.4 Correlation And Integration

Estimate: 7-10 hours.

Tasks:

- Implement correlation groups and exposure aggregation.
- Integrate portfolio/risk into trading executor and API.
- Cache current risk/portfolio snapshots in Valkey.

Acceptance:

- Trading module cannot bypass portfolio/risk constraints.

## Phase 4 — Backtester

Estimate: 25-35 hours.

Goal: build event-driven backtesting using the same live scanner/strategy/risk/trading engine.

### 4.1 Backtest Replay Engine

Estimate: 6 hours.

Tasks:

- Implement bar/event replay by symbol/timeframe.
- Feed the same indicators and strategy engine used by live scanner.
- Support deterministic execution order.

Acceptance:

- Same OHLCV input can produce identical live-style signal sequence.

### 4.2 Data Loader

Estimate: 4 hours.

Tasks:

- Load CSV, API, and cached historical data.
- Store/reuse downloaded history through Postgres or file cache.
- Validate candle continuity.

Acceptance:

- Backtest can run from a fixture without network.

### 4.3 Simulated Exchange, Portfolio, Risk

Estimate: 9 hours.

Tasks:

- Implement simulated exchange fills, fees, slippage, OCO, stops.
- Use the same portfolio/risk logic as live.
- Persist backtest order/fill/position stream under a backtest run ID.

Acceptance:

- Manual walkthrough fixture matches simulated fills and PnL.

### 4.4 Metrics, Reports, Optimizer, CLI

Estimate: 10-16 hours.

Tasks:

- Calculate Sharpe, Sortino, max drawdown, win rate, profit factor, expectancy.
- Generate terminal, JSON, and CSV reports.
- Implement grid/random/walk-forward optimization.
- Complete backtest CLI flags.

Acceptance:

- Backtest reports are persisted and exportable.
- Optimizer stores parameter sets and metrics.

## Phase 5 — Testing, Polish, Deployment

Estimate: 15-20 hours plus 10-16 hour buffer.

Goal: make the system deployable and maintainable.

Tasks:

- Expand unit tests for all indicators, strategy layers, risk, portfolio, exchange, backtest.
- Add integration tests for full pipeline: data -> indicators -> strategy -> alert/trading.
- Add API/storage/cache/runtime tests.
- Add Dockerfile and compose stack for app + Postgres + Valkey.
- Add systemd unit example.
- Add deployment docs and runbooks.
- Add seed/test fixtures and sample configs.

Acceptance:

- `cargo fmt`, `cargo test`, and `cargo build --release` pass.
- Docker compose starts the service, Postgres, and Valkey.
- `/health`, `/ready`, and `/metrics` work in local stack.
- README/manual/requirements reflect actual commands and modules.

## Cross-Phase Backlog

These tasks should be done opportunistically when touching related modules:

- Replace stringly typed config fields with enums where safe: trading mode, order type, exchange platform.
- Add `Arc<AppConfig>` or service state sharing to avoid cloning config for every symbol task.
- Add typed IDs with `uuid` for signals, orders, jobs, backtest runs, and audit events.
- Add structured error types for library/domain errors.
- Add fixtures converted from Pine Script references.
- Add benchmark harness for scanner cycle time and indicator throughput.
- Add secret handling documentation and `.env.example`.
- Add CI checks once the repository is under git.

## Milestone Gates

### Gate A — Service Shell Ready

- `serve`, `check-config`, `migrate`, `scan-once` commands exist.
- Axum health/readiness/config routes work.
- Postgres and Valkey clients can be enabled/disabled by config.
- Graceful shutdown works.

### Gate B — Scanner Ready

- Real candles flow through indicators and strategy.
- Signals are cached, persisted, and alert jobs are queued.
- `scan` runs continuously with bounded concurrency.

### Gate C — Paper Trading Ready

- Paper exchange supports realistic fills.
- Risk and portfolio can veto trades.
- Orders/fills/positions persist.
- Backtest/live engine parity assumptions are documented and tested.

### Gate D — Live Trading Candidate

- Binance testnet passes exchange tests.
- Startup reconciliation gates trading.
- Kill switch persists and blocks execution.
- Operator API can disable trading.

### Gate E — Production Candidate

- Full integration tests pass.
- Backtester produces reports.
- Docker compose stack works.
- Operational docs and runbooks are complete.

