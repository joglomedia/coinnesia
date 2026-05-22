# Architecture Audit

## Verdict

The current codebase now has the production runtime foundation and core scanner pipeline in place. It is a functioning 24/7 service shell with Axum, Postgres, Valkey, scanner/alert supervision, real market-data adapters, and persisted signal/alert flows. The remaining work is the trading, portfolio, risk, and backtesting expansion.

It now has:

- a long-running service runtime
- a web API layer
- persistent storage
- cache/pub-sub infrastructure
- operational observability and recovery
- real market-data ingestion
- persisted signal and alert pipelines

## What Is Good Already

- Clear module separation for indicators, strategy, assets, exchange, risk, portfolio, trading, backtest, alerts, and scanner.
- Config-driven shape with TOML.
- Deterministic indicator foundations for EMA, ATR, and RSI.
- Exchange abstraction exists, which prevents hard-coding vendor SDKs into core logic.
- ATR-based entry planning is present.
- Scanner ingestion, strategy scoring, caching, Postgres persistence, and alert delivery are now wired end-to-end for the signal-only path.
- `tracing` is already used, which is the right logging base for a service.

## Gaps To Fix

### 1. 24/7 service kernel exists, but trading runtime still needs completion

`main.rs` now boots the application state and service modes, and `app/` owns startup, shutdown, health, and worker supervision. The remaining runtime work is the full live/paper trading execution path.

### 2. Web API exists, but read surfaces remain partial

Axum is now in place for `/health`, `/ready`, `/metrics`, `/config`, and authenticated `POST /scan`.

Recommended role of the API:

- operational control
- status inspection
- manual rescans
- trading enable/disable
- risk and portfolio visibility
- alert history

### 3. Persistent database exists

Postgres migrations, repositories, signal persistence, alert jobs, and delivery attempts are now implemented.

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

### 4. Cache / fast state layer exists

Valkey now stores scan snapshots, signal snapshots, locks, dedupe markers, pub/sub events, and rate-limit helpers.

Why Valkey:

- fast symbol/session/scan state
- shared runtime snapshots
- distributed locks
- pub/sub for events
- rate-limit tokens
- cached indicator snapshots

### 5. Scanner pipeline exists

The scanner now fetches market data, prefetches proxies once per cycle, scores signals, caches snapshots, persists evaluations, and enqueues alert jobs.

Remaining work:

- improve long-lived publisher backpressure handling for heavier write loads
- expand read APIs for signals and scan history

### 6. Trading engine is still a shell

The trading module must become an execution service, not just a type container.

Required change:

- add order state machine
- persist order lifecycle
- reconcile exchange state on startup and periodically
- support paper/live parity
- support partial fills, cancel/replace, and stale order cleanup
- enforce risk approval before any exchange call

### 7. Portfolio and risk still need persistent runtime integration

Portfolio and risk logic must survive restarts.

Required change:

- persist positions, balances, and exposure snapshots in Postgres
- keep fast lookup and locks in Valkey
- make kill switch state durable
- add rehydration on startup

### 8. Alerting queue exists, but delivery semantics still need trading integration

Telegram alert jobs are queued, sent asynchronously, persisted, and deduplicated. The remaining work is richer alert routing and operator controls.

### 9. Observability exists, but needs more domain metrics

For a 24/7 server, you need more than logs.

Required change:

- structured tracing already exists, keep it
- add metrics endpoints
- add internal counters for scans, API latency, exchange latency, alert success, order failures, and kill-switch trips
- add uptime and last-success timestamps

### 10. Config and secrets are partially handled

TOML config, env-driven secrets, and sanitized config output exist. Remaining work is tighter config validation and reload strategy.

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

- batch market-data requests ✅ done
- reuse buffers where possible
- avoid unnecessary cloning in scan loops
- keep floats in the indicator path and convert to decimal at execution boundary
- use bounded task concurrency ✅ done
- cache proxy symbols per cycle ✅ done
- cache indicator outputs keyed by symbol/timeframe/candle hash
- rate-limit Binance HTTP requests ✅ done (Phase 1.5)
- retry transient HTTP/WS errors with backoff ✅ done (Phase 1.5)
- per-symbol data source routing ✅ done (Phase 1.6)

## HFT Capabilities (Phase 1.5 — Complete)

The following enhancements were implemented in Phase 1.5 (2026-05-22) to reduce data latency and improve resilience:

### In-Process Rate Limiter
`src/data/retry.rs` — `RateLimiter` sliding-window token bucket. Wired to `BinanceDataSource` via `config.exchange.rate_limit_per_second`. Prevents HTTP 429 errors under burst conditions.

### Retry with Exponential Backoff
`src/data/retry.rs` — `with_retry(config, op)` helper. All three HTTP adapters (Binance, TradingView, Yahoo) wrap their fetch calls. Config: `[data_sources.retry]` — `max_retries`, `base_delay_ms`, `max_delay_ms`. Permanent 4xx errors are not retried.

### Binance WebSocket Stream Client
`src/data/stream.rs` — `CandleStream` trait + `CandleEvent` type.
`src/data/binance_ws.rs` — `BinanceWsStream` combined-stream client:
- Connects to `wss://stream.binance.com/stream?streams=...` (no credentials required for public klines)
- Per-symbol ring buffer (configurable size)
- Broadcasts `CandleEvent { is_closed }` via `tokio::sync::broadcast`
- Auto-reconnect with exponential backoff
- `prime()` seeds buffer with historical REST candles before stream loop

### Event-Driven Scanner Path
`Scanner::run_streaming()` — subscribes to `CandleStream`, evaluates only closed-bar events.
`run_scanner_worker` in supervisor — routes to streaming path when `data_sources.scanning_mode = "streaming"`, falls back to polling with warning if WebSocket is disabled.

**Latency comparison:**

| Mode | Signal latency | Trigger |
|---|---|---|
| `polling` (default) | ~60 seconds | `tokio::time::interval` |
| `streaming` | <200ms | Binance kline closed event |

To enable streaming mode, set in `config/default.toml` or your config file:
```toml
[data_sources]
scanning_mode = "streaming"

[exchange.binance.ws]
enabled = true
```

### Per-Symbol Data Source Routing (Phase 1.6 — Complete)
`PerSymbolMarketData` (`src/data/mod.rs`) replaces the global `ConfiguredMarketData` as the default data source adapter for all scanner code. Each symbol is routed to its own adapter based on `SymbolConfig.data_source` (or inferred from `exchange`). Proxy symbols use `ProxySymbolEntry` with separate TradingView/Yahoo symbols and automatic fallback. `batch_candles` runs source groups concurrently via `tokio::join!`.

## Priority Order

1. Complete live/paper trading execution around the existing exchange trait.
2. Add richer read APIs for signals, orders, positions, portfolio, and risk state.
3. Expand portfolio and risk persistence/reconciliation.
4. Complete backtester and optimizer on the shared engine.
5. Add benchmark and load harnesses for scan throughput.

## Conclusion

The current architecture has cleared the Phase 0 runtime foundation and Phase 1 core scanner requirements. It is ready to move into Phase 2 trading/exchange execution work, with the main residual risks concentrated in live order safety, portfolio/risk reconciliation, richer operational APIs, and benchmarked scanner throughput.
