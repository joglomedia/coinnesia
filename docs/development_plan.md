# Execution Development Plan

This plan turns the development timeline in `docs/requirements.md` into an implementation sequence for the current source tree.

## Current Implementation Audit

Status after completing Phase 0 and Phase 1:

- CLI commands for `check-config`, `serve`, `migrate`, `scan-once`, `scan`, `trade`, and `backtest`
- Axum API routes for `/health`, `/ready`, `/metrics`, `/config`, and authenticated `POST /scan`
- supervised 24/7 service kernel with graceful shutdown, scanner, alert, trading, and reconciliation workers
- optional Postgres pool, migration runner, base schema, and repositories for core durable records
- optional Valkey client with key namespace, TTL JSON/string helpers, dedupe, locks, pub/sub, rate-limit buckets, and heartbeat helpers
- observability model with component health, heartbeat staleness, readiness, and Prometheus-compatible counters
- market data ingestion through the internal `MarketDataSource` trait with Binance HTTP klines, TradingView via `tvdata-rs`, Twelve Data REST, quotes, batch candles, and proxy prefetch
- base domain types for assets, timeframes, and candles
- deterministic indicator modules for EMA, ATR/RMA, RSI, ADX/DMI, MACD, VWAP, volume, candle shape, SMC, liquidity sweeps, order blocks, support/resistance, and regime classification
- six-layer strategy evaluation with asset profile weights, confidence scoring, timeframe thresholds, session gating, trap guard, regime blocking, and ATR-based EW/TP/SL
- scanner pipeline split into ingestion, analysis, and publishing, with bounded symbol concurrency
- Valkey latest scan/signal snapshots and signal pub/sub publication
- Postgres signal evaluation persistence through a bounded publisher worker and queued Telegram alert jobs
- Telegram alert worker that claims queued jobs, sends via Telegram Bot API, persists delivery attempts, and deduplicates repeated signal alerts through Valkey TTL keys
- exchange trait plus paper exchange stub
- placeholder/shell modules remain for full live trading, portfolio, risk expansion, and event-driven backtesting

Remaining major architecture pieces after Phase 1:

- expanded exchange trait and live Binance trading adapter
- full paper/live trading engine with order lifecycle, fills, scaling, and reconciliation
- portfolio and risk modules wired into trading execution
- event-driven backtester with reports and optimizer
- read APIs for signals, positions, orders, portfolio, and risk state
- benchmark harness for scanner cycle time and indicator throughput

## Phase 0 And Phase 1 Audit Result

Audit date: 2026-05-21.

| Area | Status | Evidence |
|---|---|---|
| CLI and module scaffold | Complete | `src/main.rs`, `src/app`, `src/api`, `src/storage`, `src/cache`, `src/observability` |
| Axum API skeleton | Complete | `/health`, `/ready`, `/metrics`, `/config`, authenticated `/scan`, route/auth tests |
| Service kernel and supervision | Complete | `AppState`, cancellation-token shutdown, supervisor workers, startup gate |
| Postgres foundation | Complete | migration `20260520000000_phase_0_4_foundation.sql`, repositories, storage integration tests |
| Valkey foundation | Complete | key builder, TTL helpers, JSON helpers, dedupe, locks, pub/sub, rate-limit buckets, cache tests |
| Observability | Complete | health/readiness model, heartbeat staleness, metrics endpoint and counters |
| Market data ingestion | Complete | `MarketDataSource`, Binance HTTP, TradingView `tvdata-rs`, Twelve Data REST, proxy prefetch |
| Indicator completion | Complete | deterministic modules and fixture/parity tests for Phase 1 indicator set |
| Strategy engine | Complete | six-layer evaluation, confidence scoring, MTF thresholds, session/regime/trap blocking |
| Scanner pipeline | Complete | ingestion/analysis/publishing split, bounded concurrency, Valkey snapshots, Postgres signal persistence, alert job enqueue |
| Telegram alert worker | Complete | queued job claim, Telegram Bot API sender, delivery attempts, Valkey TTL dedupe, TP3 optional tests |

Known non-blocking Phase 2+ gaps: full trading execution, live Binance account/order adapter, portfolio/risk execution integration, richer read APIs, event-driven backtester, and benchmark harness.

## Phase 1.5 — Resilience & HFT Foundation

Estimate: 12–16 hours.

Status: **Complete**. Implemented 2026-05-22.

Goal: Harden the data ingestion layer with retry logic and in-process rate limiting to eliminate unhandled HTTP 429 / transient failure scenarios, and upgrade the scanner to support event-driven WebSocket streaming as an alternative to interval-based REST polling. These changes drop observable signal latency from ~60 seconds to sub-200ms for closed-candle events on supported Binance symbols.

### 1.5.1 In-Process Rate Limiter

Estimate: 2 hours. Status: **Complete**.

Files: `src/data/retry.rs` (`RateLimiter`).

Tasks:

- Implement `RateLimiter` as a sliding-window token bucket backed by `Arc<Mutex<...>>`.
- Wire to `BinanceDataSource` via `config.exchange.rate_limit_per_second`.
- `acquire()` blocks the caller until a slot is available in the current window; it never drops requests.

Acceptance:

- Binance adapter acquires a rate-limit slot before every HTTP klines request.
- No HTTP 429 errors under burst conditions when `rate_limit_per_second` is honoured.

### 1.5.2 Retry with Exponential Backoff

Estimate: 2 hours. Status: **Complete**.

Files: `src/data/retry.rs` (`with_retry`), `src/data/binance.rs`, `src/data/tradingview.rs`, `src/data/twelvedata.rs`.

Tasks:

- `with_retry(config, operation)` generic helper with exponential backoff and deterministic jitter.
- `RetryConfig { max_retries, base_delay_ms, max_delay_ms }` added to `[data_sources.retry]` in TOML.
- Permanent HTTP 4xx errors (except 429) are not retried to avoid unnecessary delay.
- All HTTP adapters (Binance, TradingView, Twelve Data) wrap their fetch calls in `with_retry`.

Acceptance:

- A transient HTTP 500 or connection reset is retried up to `max_retries` times before propagating.
- HTTP 400 errors fail immediately without retry.
- `cargo test -- --test-threads=1` passes with `max_retries = 0` in test configs.

### 1.5.3 Binance WebSocket Stream Client

Estimate: 6 hours. Status: **Complete**.

Files: `src/data/stream.rs`, `src/data/binance_ws.rs`.

Tasks:

- `CandleStream` trait in `src/data/stream.rs`: `subscribe()`, `prime()`, `candles()`, `symbols()`, `run()`.
- `BinanceWsStream` in `src/data/binance_ws.rs` implements `CandleStream`:
  - Connects to Binance combined-stream endpoint (`wss://stream.binance.com/stream?streams=...`).
  - Maintains a per-symbol ring buffer (size: `candle_buffer_size`, default 500).
  - Broadcasts `CandleEvent { symbol, timeframe, candle, is_closed }` via `tokio::sync::broadcast`.
  - Auto-reconnects with exponential backoff (`reconnect_base_delay_ms` → `reconnect_max_delay_ms`).
  - Splits symbol list into chunks of `max_streams_per_connection` (default 200) for multiple connections.
  - `prime()` seeds buffer with historical candles before stream loop enters.
  - Graceful shutdown via `CancellationToken`.
- `data::binance_ws_stream(config)` factory in `src/data/mod.rs` builds the stream if enabled.
- New Cargo dependency: `tokio-tungstenite = { version = "0.24", features = ["rustls-tls-webpki-roots", "connect"] }`.

Acceptance:

- WebSocket connects to Binance public streams without API credentials.
- Closed-candle events arrive within one kline interval of bar close.
- Reconnects automatically after a dropped connection.
- Shutdown via cancellation token completes cleanly.

### 1.5.4 Event-Driven Scanner Path

Estimate: 3 hours. Status: **Complete**.

Files: `src/scanner/mod.rs` (`run_streaming`, `handle_streaming_event`), `src/app/supervisor.rs` (`run_scanner_worker`).

Tasks:

- `Scanner::run_streaming(stream, shutdown)`:
  - Seeds stream buffers with historical candles via REST before subscribing.
  - Subscribes to the broadcast channel.
  - Ignores intra-bar updates (`is_closed = false`); processes only closed candles.
  - Calls `analyze()` and `publish()` per symbol event — identical to the polling path.
- `run_scanner_worker` in `supervisor.rs` checks `config.data_sources.scanning_mode`:
  - `"streaming"`: calls `run_streaming` with a `BinanceWsStream` if enabled, falls back to polling with a warning.
  - `"polling"` (default): existing `tokio::time::interval` loop unchanged.

Acceptance:

- `scanning_mode = "streaming"` + `exchange.binance.ws.enabled = true` → event-driven scan.
- `scanning_mode = "polling"` (default) → existing 60-second interval scan; all existing tests pass.
- Log line `"streaming mode enabled via Binance WebSocket"` visible at startup in streaming mode.

### 1.5.5 Config and Documentation

Estimate: 1 hour. Status: **Complete**.

New config keys in `config/default.toml`:

```toml
[data_sources]
scanning_mode = "polling"   # "polling" | "streaming"

[data_sources.retry]
max_retries = 3
base_delay_ms = 500
max_delay_ms = 10000

[exchange.binance.ws]
enabled = false
url = "wss://stream.binance.com/stream"
max_streams_per_connection = 200
reconnect_base_delay_ms = 1000
reconnect_max_delay_ms = 30000
candle_buffer_size = 500
```

Updated docs: `AGENTS.md` (rules 18–21), `docs/architecture_audit.md` (gaps + HFT capabilities), `docs/requirements.md` (streaming section), `docs/manual.md` (WebSocket streaming configuration guide).


## Phase 1.6 — Per-Symbol Data Source Routing

Estimate: 4–6 hours.

Status: **Complete**. Implemented 2026-05-22.

Goal: Allow each symbol to declare its preferred data source independently, enabling Forex/stocks symbols to use TradingView, crypto to use Binance, and proxy symbols (XAUUSD, IHSG, DXY) to use TradingView or Twelve Data. Replaces the global `ConfiguredMarketData` with `PerSymbolMarketData` which also runs source groups concurrently in `batch_candles` for better throughput.

## Phase 1.7 — Pine Script Parity & Panel Report

Estimate: 80–110 hours.

Status: **Planned**. Audit date 2026-05-25. Detailed plan in `docs/phase1_pine_parity_plan.md`.

Goal: bring scanner/strategy output to functional parity with the TradingView Pine reference panels for all five asset classes. The 2026-05-25 audit found that indicator math (RSI/ATR/ADX/EMA/MACD) matches Pine but the higher-level composition is missing most of the V61.4 → V62.0 protective stack, the MTF feed, asset-specific evaluator branching, proxy snapshot plumbing, and the full panel report data structure. Current panel parity is ~15–20%.

Headline sub-phases (full breakdown in the parity plan doc):

- 1.7.1 Core strategy bug fixes (regime gate, RSI/MACD layers, EW1 clamp, OB body-ratio, zone tolerance).
- 1.7.2 Session & VWAP boundary alignment to Pine WIB cutoffs.
- 1.7.3 Multi-timeframe data pipeline (M1/M5/M15/H1/H4/D1/W1/MN) + consensus + microTrend.
- 1.7.4 Stateful guard counters (trap cooldown, shock freeze, deep reclaim, SMC trend state).
- 1.7.5 EW/SL/TP engine rewrite — swing/VWAP/EMA anchored, session-reachable, liquidity-capped, flow-adaptive, probability-scored. **Done 2026-05-26.**
- 1.7.6 Trap guard & V61.8 flow engine. **Done 2026-05-28.**
- 1.7.7 New indicators: CMF, OBV, RVOL, HTF bias, relative-strength. **Done 2026-05-28.**
- 1.7.8 Asset-class evaluator branching (Gold proxy bias, Forex HTF, IDX RVOL/CMF/OBV/RS, Altcoin V62 adaptive engine).
- 1.7.9 Proxy snapshot plumbing through scanner. **Done 2026-05-28.**
- 1.7.10 `PanelReport` struct (TRADE SCORE, BIAS, FLOW, EW status, DEEP RISK status, TRAP GATE, ENTRY IDEAL, WAKTU ENTRY, SL width, TP probability, ETA TP1-3, RECLAIM).
- 1.7.11 Alert & API surfaces consuming the panel.
- 1.7.12 Configuration additions (V61.4-V62.0 knobs + per-asset overrides).
- 1.7.13 Parity test harness against captured Pine fixtures.

### Changes

- `SymbolConfig.data_source: Option<String>` — explicit override per symbol; defaults derived from `exchange` field.
- `ProxySymbolEntry` — replaces flat `ProxySymbols` strings with structured `{ tradingview, twelvedata, source }` fields.
- `PerSymbolMarketData` — new primary data source adapter:
  - Routing table built once at startup from config
  - `candles()`: routes to configured adapter + Twelve Data fallback if empty
  - `batch_candles()`: groups by source, runs Binance/TV/Twelve Data concurrently via `tokio::join!`, then merges
- `TradingViewDataSource::batch_candles` (`src/data/tradingview.rs`) — adds a second concurrency tier: requests are sub-grouped by `(timeframe, limit)` and all sub-groups run concurrently via `futures::future::try_join_all` over a single shared `Arc<TradingViewClient>` (chart WebSocket transport from `tvdata-rs`, `download_history_map`). Direct `BTreeMap<Ticker, HistorySeries>` lookup replaces the earlier O(n²) linear scan, and symbols missing from the WS response emit `tracing::debug!` instead of silently mapping to an empty vec.
- `TwelveDataDataSource::batch_candles` (`src/data/twelvedata.rs`) — overrides the default sequential trait impl and fans out one REST `/time_series` request per `CandleRequest` concurrently via `futures::future::try_join_all`, sharing the same `reqwest::Client` and the `retry::with_retry` wrapper. Twelve Data's WebSocket (`wss://ws.twelvedata.com/v1/quotes/price`) only streams live tick prices and cannot serve historical OHLCV, so REST stays the transport — the win is parallelizing the round-trips, not switching protocols.
- `proxy.rs` updated to use `ProxySymbolEntry.symbol()` as request key
- All scanner, supervisor, and CLI entry points migrated from `ConfiguredMarketData` to `PerSymbolMarketData`
- `ProxySymbolEntry::from_twelvedata()` helper for tests

### Acceptance

- `cargo test -- --test-threads=1` passes
- `cargo run -- check-config` succeeds with new proxy_symbols TOML format
- BTCUSDT routes to Binance; proxy symbols route to TradingView with Twelve Data fallback

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

Status: **Complete**. Implemented in `src/app`, `src/api`, `src/storage`, `src/cache`, `src/observability`, `src/main.rs`, and covered by unit/integration tests.

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

- `serve` starts Axum and supervised scanner, alert, trading, and reconciliation workers.
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

Status: **Complete**. `scan-once` and `scan` now fetch candles, run the strategy engine, cache/persist signal outputs, enqueue alert jobs, and the supervised alert worker delivers/deduplicates Telegram messages when enabled.

### 1.1 Market Data Ingestion

Estimate: 5 hours.

Tasks:

- Expand `MarketDataSource` to support batch candles and quotes.
- Implement Binance crypto OHLCV fetch.
- Implement Twelve Data Finance fallback for daily/proxy data.
- Implement TradingView adapter with `tvdata-rs` behind the internal `MarketDataSource` trait; keep auth optional and config/env-driven.
- Keep `tail-fin-tradingview` as an optional later adapter for live streaming/Pine catalog tooling if `tvdata-rs` cannot satisfy a required feature.
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
