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
`src/data/retry.rs` — `with_retry(config, op)` helper. All HTTP adapters (Binance, TradingView, Twelve Data) wrap their fetch calls. Config: `[data_sources.retry]` — `max_retries`, `base_delay_ms`, `max_delay_ms`. Permanent 4xx errors are not retried.

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
`PerSymbolMarketData` (`src/data/mod.rs`) replaces the global `ConfiguredMarketData` as the default data source adapter for all scanner code. Each symbol is routed to its own adapter based on `SymbolConfig.data_source` (or inferred from `exchange`). Proxy symbols use `ProxySymbolEntry` with separate TradingView and Twelve Data symbol identifiers. `batch_candles` runs source groups concurrently via `tokio::join!`.

Within the TradingView adapter, `TradingViewDataSource::batch_candles` (`src/data/tradingview.rs`) further parallelizes by `(timeframe, limit)` group using `futures::future::try_join_all` over a single shared `Arc<TradingViewClient>`. The underlying `tvdata-rs::download_history_map` opens one chart WebSocket session per call and pipelines all symbols through it with bounded internal concurrency, so a typical multi-timeframe scan (M15/H1/H4/D1) costs ~1 WS round-trip-time instead of 4. Per-symbol lookup against the returned `BTreeMap<Ticker, HistorySeries>` is direct; symbols missing from the response emit a `tracing::debug!` rather than being silently substituted with an empty series.

## Priority Order

1. Complete live/paper trading execution around the existing exchange trait.
2. Add richer read APIs for signals, orders, positions, portfolio, and risk state.
3. Expand portfolio and risk persistence/reconciliation.
4. Complete backtester and optimizer on the shared engine.
5. Add benchmark and load harnesses for scan throughput.

---

## Scan Pipeline — Detailed Architecture

This section documents the end-to-end data flow of one scan cycle, as verified against the
running implementation (`scan-once` command output, 2026-05-23).

### Call graph

```
main.rs: Command::ScanOnce
└── Scanner::scan_once()                              scanner/mod.rs:73
    ├── Scanner::ingest()                             scanner/mod.rs:89
    │   ├── proxy::fetch_once_per_cycle()             data/proxy.rs:20
    │   │   └── PerSymbolMarketData::batch_candles()  data/mod.rs:205
    │   │       ├── TradingViewDataSource::batch_candles()  data/tradingview.rs:132
    │   │       │   └── try_join_all over (timeframe, limit) groups
    │   │       │       └── tvdata-rs::download_history_map()  (chart WebSocket, shared client)
    │   │       └── .unwrap_or_default()              (proxy failure = non-fatal)
    │   └── PerSymbolMarketData::batch_candles()      data/mod.rs:205
    │       └── BinanceDataSource::candles()  ×N      data/binance.rs (via binance-sdk REST)
    ├── Scanner::analyze()                            scanner/mod.rs:133
    │   └── [per symbol, tokio::spawn, bounded semaphore]
    │       └── StrategyEngine::evaluate()            strategy/mod.rs:23
    │           └── SignalGenerator::evaluate()       strategy/signals.rs:66
    │               ├── IndicatorSnapshot::new()      (all 14 indicators)
    │               ├── Layer 1: classify_regime()    indicators/regime.rs
    │               ├── Layer 2: session_allows_asset()  strategy/session.rs
    │               ├── Layer 3–4: evaluate_direction() ×2  strategy/signals.rs
    │               │   └── ConfidenceScore::from_sides()   strategy/confidence.rs
    │               ├── Layer 5: evaluate_trap_guard()  strategy/trap_guard.rs
    │               ├── Layer 6: directional_gap check
    │               └── EntryPlanCalculator::calculate()  strategy/entry_plan.rs
    └── ScanPublisher::publish()                      scanner/mod.rs:305
        ├── cache_snapshots() → Valkey               (ScanSnapshot, SignalSnapshot)
        ├── signal_record() → Postgres signal_evaluations
        ├── alert_job() → Postgres alert_jobs
        └── cache.publish_json("signals", …) → Valkey pub/sub
```

### Signal state machine

```
candles < min_length         → Wait  (not_enough_candles)
candles empty                → Wait  (market_data_unavailable)
regime = Shock               → Freeze (shock_regime)
regime ∈ {Sideways, Chop}   → Wait  (regime_block:*)
session blocks asset class   → Wait  (session_block:*)
both sides trap-blocked      → Wait  (trap_guard_blocked)
confidence < threshold       → Wait  (layers_not_met)
gap < min_directional_gap    → Wait  (layers_not_met)
long wins and all pass       → Long  (six_layer_pass) + EntryPlan
short wins and all pass      → Short (six_layer_pass) + EntryPlan
```

### Observed output (2026-05-23, BTCUSDT M15 / ETHUSDT M15 / PAXGUSDT H1)

```
BTCUSDT  Wait  long=0.0  short=55.0  gap=55.0
         layers_not_met threshold=72.0 (short needs +17 to trigger)

ETHUSDT  Wait  long=0.0  short=0.0   gap=0.0
         regime_block:Sideways (stopped at Layer 1)

PAXGUSDT Wait  long=0.0  short=0.0   gap=0.0
         regime_block:Sideways (stopped at Layer 1)
```

`signals=3` in the log summary always equals `scanned` — it is the count of evaluations, not
actionable trade opportunities.

---

## Data Source Operational Status

### Binance REST (klines) — Functional

- Public endpoint, no API key required.
- Implemented via `binance-sdk v50` (Phase 1.5, 2026-05-22).
- Client: `BinanceDataSource` wraps `SpotRestApi::from_config()` with:
  - In-process `RateLimiter` (sliding window, default 10 req/s)
  - `retry::with_retry` with exponential backoff (SDK retries disabled via `retries=1`)
  - 10-second timeout (SDK default 1s was too short for SEA → AWS latency)
- SDK `KlinesItemInner` enum replaces fragile `serde_json::Value` positional indexing.

### Binance WebSocket — Functional (disabled by default)

- `BinanceWsStream` in `src/data/binance_ws.rs`.
- Uses `binance-sdk::spot::websocket_streams::KlineResponse` for typed message deserialization.
- Reconnect loop, ring buffer, and broadcast channel remain bespoke (application-level logic).
- Enable via `[exchange.binance.ws] enabled = true` + `scanning_mode = "streaming"`.

### TradingView (tvdata-rs) — Functional with session auth

- Required for proxy symbols (XAUUSD, IHSG, DXY) when `source = "tradingview"`.
- Authentication via `sessionid` cookie sent in WebSocket `Cookie` header.
- `TRADINGVIEW_SESSION_ID` (required) + `TRADINGVIEW_SESSIONID_SIGN` (recommended).
- `TRADINGVIEW_AUTH_TOKEN` is the JWT for the `set_auth_token` WebSocket message.
  When session is set, the library defaults to `"unauthorized_user_token"` — the auth token
  env is **optional** and should be omitted unless a proper user-session JWT is available.
  (A chart-share JWT with `iss: "tv_chart"` is not a valid user auth token.)
- Session cookies expire in 2–4 weeks. Renew via browser DevTools on `tradingview.com`.
- Transport: `tvdata-rs::TradingViewClient` constructed once from `TradingViewClientConfig::backend_history()` and cached in `Arc<OnceLock<…>>`. `client.history()` and `client.download_history_map()` both run over TradingView's chart WebSocket (`TradingViewWebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>`); REST is not used for OHLCV.
- Batch fetch: `batch_candles` groups requests by `(timeframe, limit)` and drives all groups concurrently with `futures::future::try_join_all`. Within each group, `download_history_map` parallelizes symbols using the SDK's bounded `request_budget`. Per-symbol lookup uses direct `BTreeMap` access; symbols missing from the response are logged at `debug` level and resolved to an empty candle vec.

### Twelve Data — Functional (free API key required)

- Implemented in `src/data/twelvedata.rs` as `TwelveDataDataSource`.
- Free tier: 800 API credits/day. Recommended `scan_interval_secs = 300` for free tier users.
- No browser session required — static API key via `TWELVE_DATA_API_KEY` env var.
- Supports all 8 timeframes (M1 through Mn1).
- Replaces Yahoo Finance for proxy symbols (XAUUSD, DXY, IHSG).

### Yahoo Finance — Removed

Yahoo Finance was previously used as a fallback data source for proxy symbols (XAUUSD, DXY, IHSG).
It was removed because Cloudflare Bot Protection blocks all `/v8/finance/chart/*` endpoints
(HTTP 429) unconditionally, making the adapter non-functional without a browser session.

Twelve Data is the replacement. The `YahooDataSource`, `YahooDataSourceConfig`, `yahoo` field in
`ProxySymbolEntry`, and `yahoo_interval()` helper were deleted in their entirety.

---

## Conclusion

The current architecture has cleared the Phase 0 runtime foundation and Phase 1 core scanner
requirements. It is ready to move into Phase 2 trading/exchange execution work, with the main
residual risks concentrated in live order safety, portfolio/risk reconciliation, richer operational
APIs, and benchmarked scanner throughput.

