# AGENTS.md — LLM Development Guide for coinnesia

This file provides context and instructions for any LLM assistant working on this project.

## Project Summary

A high-performance Rust trading bot, signal scanner, and asset/portfolio manager that monitors technical indicators across multiple asset classes, delivers real-time alerts via Telegram, exposes a CLI and Axum web API, and can run 24/7 as a supervised service. The bot ports TradingView Pine Script v6 strategies into a fast async Rust implementation while preserving a shared live/paper/backtest engine.

**Language**: Rust (2021 edition)
**Async runtime**: Tokio
**Web API**: Axum
**Database**: Postgres
**Cache / pub-sub / locks**: Valkey (Redis-compatible)
**Target**: Scan 500+ symbols in ~2.5s per cycle and operate continuously as a resilient trading service

## Documentation

Read these before making changes:

- `docs/requirements.md` — Full architecture, indicators, strategy, EW/TP/SL engine, asset profiles
- `docs/indicators.md` — Detailed indicator analysis per asset class (Indonesian language)
- `docs/architecture_audit.md` — Production architecture audit for 24/7 service mode, Axum API, Postgres, Valkey, and operational gaps
- `docs/TV_Pine_Scripts/*.pine.txt` — Reference Pine Script v6 implementations (5 scripts)

## Asset Classes & Design Philosophy

Each asset class has a distinct signal priority. Never apply one asset's logic to another without checking the profile:

| Asset | Philosophy | Primary Indicators |
|---|---|---|
| BTC | Structure first | HTF BOS/CHOCH → EMA → VWAP → ADX |
| Altcoin | Anti-trap first | LTF consensus → wick chaos → volume → ATR |
| Gold PAXG/XAUT | Proxy first | XAUUSD direction → H1/H4/D1 → London/USA session |
| Forex | Session + RR first | HTF bias → session → structure → ADX |
| Stocks IDX | Volume + guard first | RVOL → IHSG benchmark → EMA → CMF/OBV |

## Architecture Rules

1. **Modular indicators** — Each indicator is its own module with a common trait. Never mix indicator logic across modules.
2. **Asset profiles drive weights** — Indicator weights are loaded from TOML config per asset class. Never hardcode asset-specific behavior in core indicator code; put it in the asset adapter module.
3. **Six-layer signal evaluation** — Signals must pass: Trend → Momentum → Volume → Entry Trigger → Anti-Trap → Regime/Session. Do not shortcut layers.
4. **States are LONG/SHORT/WAIT/FREEZE** — WAIT means conditions not met. FREEZE means dangerous market (shock/liquidation). Never generate signals during FREEZE.
5. **Confidence scoring** — Each indicator contributes a weighted score. Signal fires only when total confidence exceeds the timeframe threshold AND directional gap exceeds minimum.
6. **Exchange abstraction** — All exchange interaction goes through the `Exchange` trait. Trading, portfolio, risk, and backtest modules never import exchange SDK crates directly.
7. **Risk before execution** — Every trade must pass the risk module before reaching the exchange. The flow is: Signal → Risk Check → Position Size → Order → Exchange.
8. **Shared engine for backtest** — The backtester uses the same indicator/strategy/risk code as live. Only data replay, simulated fills, and metrics are backtest-specific.
9. **Config-driven behavior** — All thresholds, limits, allocations, and feature flags live in TOML config. The code reads config; it never contains magic numbers for trading parameters.
10. **Three operating modes** — The system supports: scan-only (alerts), paper trading (simulated), and live trading (real). Mode is a config switch, not a code branch.
11. **24/7 service runtime** — Production operation runs under a supervised service kernel with graceful shutdown, health/readiness state, retry/reconnect policy, background workers, and periodic reconciliation.
12. **Axum web API** — Any HTTP API must use Axum. Handlers stay thin and call application services; do not put trading, strategy, or database business logic directly in handlers.
13. **Postgres for durable state** — Signals, alerts, orders, fills, positions, balances, portfolio snapshots, risk events, backtest runs, config snapshots, and audit records must be persisted in Postgres.
14. **Valkey for hot state** — Use Valkey for fast ephemeral state, scan snapshots, deduplication, pub/sub, distributed locks, rate-limit tokens, and cached indicator snapshots. Do not put durable accounting state only in Valkey.
15. **Hot path stays fast** — Indicator calculation, signal scoring, risk approval, and order routing must stay memory-first and avoid blocking database calls in per-symbol hot loops.
16. **Queued side effects** — Telegram delivery, persistence writes that are not execution-critical, and report generation should be worker/queue-driven instead of blocking the scanner hot path.
17. **Startup reconciliation** — On service startup and periodically during runtime, reconcile exchange orders, balances, and positions against Postgres state before allowing live trading.
18. **Dual-mode data ingestion** — The system supports `"polling"` (interval-based REST) and `"streaming"` (event-driven WebSocket) scanner modes. Mode is a config switch (`data_sources.scanning_mode`). Never hardcode timing assumptions (e.g., "60-second cycle") in indicator or strategy code; those assumptions only live in the scanner loop.
19. **CandleStream wraps WebSocket** — All WebSocket streaming logic must go through the `CandleStream` trait in `src/data/stream.rs`. Scanner, supervisor, and strategy modules must never import `tokio-tungstenite` directly. `BinanceWsStream` in `src/data/binance_ws.rs` is the only concrete WebSocket implementation.
20. **Retry all external I/O** — Every HTTP and WebSocket operation must be wrapped in `data::retry::with_retry()`. Never let a single transient network failure propagate unretried to the scanner hot path. Permanent HTTP 4xx errors (except 429) bypass retry.
21. **Rate-limit before Binance calls** — Acquire from `RateLimiter` before every Binance REST klines request. Config: `exchange.rate_limit_per_second`. The limiter lives in `src/data/retry.rs`; it is in-process and independent of the Valkey rate-limit bucket (which is for inter-process/distributed throttling).
22. **Per-symbol data source routing** — Use `SymbolConfig.data_source` to assign a symbol to a specific adapter (`"binance"` | `"tradingview"` | `"twelvedata"`). Proxy symbols use `ProxySymbolEntry` (see `[proxy_symbols.xauusd]` etc.) with separate TradingView and Twelve Data symbol strings and a `source` preference. The `PerSymbolMarketData` adapter reads these at startup and builds a routing table — never hard-code symbol-to-source mappings in strategy or scanner code. When `data_source` is absent on a `SymbolConfig`, the adapter derives it from the `exchange` field (binance → `"binance"`; tradingview → `"tradingview"`; else global `data_sources.primary`).

## Coding Conventions

### Rust Style

- Use `anyhow::Result` for error propagation in application code
- Use `thiserror` for library-level custom errors
- Async functions use `async fn` with Tokio runtime
- Prefer `tokio::spawn` plus bounded semaphores for concurrent symbol scanning
- Use `tracing` crate for structured logging (not `println!` or `log`)
- Configuration via `serde` + `toml` deserialization
- All indicator calculations must be deterministic given the same OHLCV input
- Use Axum for web API routes and middleware
- Use async Postgres access with a connection pool; never perform blocking database I/O in async tasks
- Use a Redis-compatible async client for Valkey; key names must include a stable project prefix
- Secrets are loaded from environment variables or secret stores, never committed in TOML defaults

### Naming

- Modules: `snake_case` (e.g., `trap_guard.rs`, `order_block.rs`)
- Structs: `PascalCase` (e.g., `SignalResult`, `EntryPlan`, `AssetProfile`)
- Functions: `snake_case` (e.g., `calculate_rsi`, `detect_liquidity_sweep`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `DEFAULT_ATR_PERIOD`)
- Config fields: `snake_case` matching TOML keys

### Module Organization

```
src/
├── app/             # 24/7 service kernel, worker supervision, graceful shutdown
├── api/             # Axum routes, request/response DTOs, auth middleware
├── config/          # TOML config + asset profiles + defaults
├── data/            # Data fetchers (TradingView, Binance, Twelve Data, proxy)
├── storage/         # Postgres repositories, migrations, durable models
├── cache/           # Valkey client, hot state, pub/sub, distributed locks
├── indicators/      # One file per indicator system
├── strategy/        # Confidence scorer, signals, trap guard, session, MTF, EW/TP/SL
├── assets/          # Asset-specific adapters (btc, altcoin, gold, forex, stocks_idx)
├── exchange/        # Exchange trait + implementations (binance, mexc, bybit, okx, paper)
├── trading/         # Order execution, position tracking, scaling
├── portfolio/       # Capital allocation, balance, exposure, rebalancing
├── risk/            # Position sizing, drawdown, limits, kill switch
├── backtest/        # Event-driven backtester, sim exchange, metrics, optimizer
├── alerts/          # Telegram formatter + sender
├── observability/   # Health/readiness, metrics, structured runtime status
└── scanner/         # Main loop orchestration + rate limiter
```

## Critical Implementation Details

### RSI Must Use RMA

RSI calculation MUST use RMA (Relative Moving Average / Wilder's smoothing), NOT SMA. This matches TradingView's implementation. Using SMA produces different values and breaks signal accuracy.

```rust
// Correct: RMA-based RSI
rma_gain = (prev_rma_gain * (period - 1) + current_gain) / period
rma_loss = (prev_rma_loss * (period - 1) + current_loss) / period
rs = rma_gain / rma_loss
rsi = 100.0 - (100.0 / (1.0 + rs))
```

### ATR Is the Distance Backbone

All distance calculations (EW, TP, SL, trap thresholds) are expressed as ATR multiples. When implementing any distance-based logic, always multiply by ATR — never use fixed price distances.

### Session Times Are in WIB (Asia/Jakarta, UTC+7)

- Asia: 06:00–14:00 WIB
- Europe/London: 14:00–22:00 WIB
- USA/New York: 19:00–03:00 WIB
- IDX market: 09:00–15:00 WIB
- Forex rollover avoid: 04:55–06:10 WIB

### Proxy Symbols Are Fetched Once Per Cycle

XAUUSD and IDX:COMPOSITE data should be fetched once at the start of each scan cycle and shared across all symbols that need them. Do not re-fetch per symbol. Use `source = "tradingview"` when a valid TradingView session is available; use `source = "twelvedata"` with a static API key as the stable alternative.

### TradingView Data Source

Use `tvdata-rs` as the primary unofficial TradingView dependency for new datasource work. It must be wrapped by `src/data/tradingview.rs` and the internal `MarketDataSource` trait; scanner, strategy, trading, risk, portfolio, and backtest modules must not import `tvdata-rs` directly.

`tail-fin-tradingview` is allowed only as an optional secondary dependency for live WebSocket streaming, Pine/catalog tooling, or operational data exploration if `tvdata-rs` cannot cover a required feature. `tradingview-rs` is legacy/backup research material and is not the preferred default dependency.

The `TradingViewClient` is constructed once from `TradingViewClientConfig::backend_history()` and cached in `Arc<OnceLock<…>>` so all callers share one chart-WebSocket-backed client. `candles()` uses `client.history()`; `batch_candles()` sub-groups by `(timeframe, limit)` and runs sub-groups concurrently via `futures::future::try_join_all`, calling `client.download_history_map()` per sub-group (one WS session, bounded internal concurrency via the SDK's `request_budget`). Do not bypass this adapter by calling `tvdata-rs` directly — that would open additional WS connections and skip retry/concurrency tiering.

TradingView auth is optional when guest/public access works. Authenticated values must be loaded from config/env vars and never committed:

```toml
[data_sources.tradingview]
enabled = false
auth_token_env = "TRADINGVIEW_AUTH_TOKEN"
session_id_env = "TRADINGVIEW_SESSION_ID"
session_signature_env = "TRADINGVIEW_SESSIONID_SIGN"
device_token_env = "TRADINGVIEW_DEVICE_T"
```

### Twelve Data REST Adapter

Twelve Data is the stable alternative data source for proxy symbols (XAUUSD, DXY, IHSG) when TradingView session cookies are unavailable or expired. It requires a static API key — no browser session needed.

```toml
[data_sources.twelvedata]
enabled     = true
base_url    = "https://api.twelvedata.com"
api_key_env = "TWELVE_DATA_API_KEY"

[proxy_symbols.xauusd]
tradingview = "OANDA:XAUUSD"
twelvedata  = "XAU/USD"
source      = "twelvedata"   # or "tradingview"
```

- Supports all timeframes M1 through Mn1 (intraday included, unlike the removed Yahoo adapter which was D1-only).
- Free tier: 800 API credits/day. Set `scan_interval_secs = 300` for free tier with 3 proxy symbols.
- Implementation: `src/data/twelvedata.rs` (`TwelveDataDataSource`).
- Data source priority per asset: Binance (crypto) → TradingView via `tvdata-rs` (all/intraday) → Twelve Data (proxy symbols, static API key).
- Transport: REST `/time_series` only. Twelve Data's WebSocket (`wss://ws.twelvedata.com/v1/quotes/price`) is a forward-only live tick-price stream and **cannot** deliver historical OHLCV bars; do not wire `TwelveDataDataSource::candles` to a WebSocket transport. (Full WS access also requires the Pro plan; REST `/time_series` works on the free tier.)
- Batch fetch: `TwelveDataDataSource::batch_candles` overrides the default sequential trait impl and fans out one REST request per `CandleRequest` concurrently via `futures::future::try_join_all`, sharing the same `reqwest::Client` and the `retry::with_retry` wrapper. Bypassing this adapter to call `/time_series` from elsewhere will skip retry/backoff and serialize what should be a single concurrent round-trip — always go through `MarketDataSource::batch_candles`.

### Trap Guard Can Block Signals

The trap guard engine can reduce confidence (trap penalty) or completely block signal generation (shock freeze, trap cooldown). This is intentional — WAIT is better than a trapped trade.

### TP3 Is Never Guaranteed

TP3 should always be marked as "optional" in alerts. It is only valid when: flow is high, ADX supports, no nearby S/R, no trap active, volume not decaying, session active.

### Exchange Trait Is the Only Way to Touch an Exchange

All order execution, balance queries, and market data from exchanges MUST go through the `Exchange` trait. Never call Binance (or any exchange) SDK directly from trading/portfolio/risk code. This ensures:
- Platform swapping is a config change, not a code change
- Paper trading works identically to live trading
- Backtester's simulated exchange plugs in seamlessly

### Backtester Must Use the Same Engine as Live

The backtester MUST import and call the same `indicators/`, `strategy/`, `assets/`, `risk/`, and `portfolio/` modules as live trading. Never duplicate indicator or strategy logic in the backtest module. The only backtest-specific code is: data loading, bar replay, simulated exchange, metrics calculation, and report generation.

### Risk Module Can Veto Any Trade

The risk manager sits between strategy signals and order execution. It can:
- Reduce position size (drawdown warning)
- Block a trade entirely (exposure limit, daily limit, kill switch)
- Force-close positions (critical drawdown)

Trading code must always check risk approval before sending orders. Never bypass the risk layer.

### Kill Switch Requires Manual Restart

When the kill switch triggers, `trading.enabled` is effectively set to `false`. The system requires a manual config flag reset or explicit restart command. This prevents the bot from resuming trading after a catastrophic event without human review.

### Position Scaling Follows EW Plan

When auto-trading, entries are scaled according to the EW plan from the strategy engine:
- EW1 = initial entry (largest portion)
- EW2/EW3 = adds on pullback (smaller portions)
- Deep Add = aggressive add (smallest portion)
- Never exceed total planned position size
- If price doesn't reach EW2/EW3, those portions stay undeployed

### Service Runtime Must Survive 24/7 Operation

Production mode must be designed as a long-running service, not a one-shot script:
- `serve` starts the Axum API plus background workers
- scanner loops run on bounded intervals with bounded concurrency
- workers use cancellation tokens or equivalent shutdown signals
- critical tasks are supervised and failures are logged with enough context to restart or degrade safely
- health/readiness endpoints report database, Valkey, exchange, data source, scanner, and alert worker status
- live trading remains disabled until startup reconciliation completes

### Postgres Is the Durable Source of Truth

Use Postgres for durable records:
- signals and signal evaluations
- alert jobs and delivery attempts
- orders, fills, and order state transitions
- positions, balances, and portfolio snapshots
- risk decisions, drawdown events, and kill switch state
- backtest runs, parameters, metrics, and reports
- config snapshots and audit events

Do not rely on in-memory state or Valkey alone for accounting, risk, position, or kill switch data that must survive restart.

### Valkey Is the Hot State Layer

Use Valkey for low-latency runtime state:
- latest candle/indicator/signal snapshots
- scan cycle status and deduplication keys
- pub/sub between scanner, alert, API, and trading workers
- distributed locks for one-active-scanner or one-active-trader semantics
- rate-limit buckets and cooldown markers

Valkey is a Redis-compatible cache, not the system of record.

### Axum API Must Stay Thin

API handlers should:
- validate/authenticate requests
- call application services
- return typed DTOs
- avoid direct strategy, exchange, or SQL business logic

Expected API surface includes health/readiness, config inspection, latest signals, positions, orders, portfolio, risk state, manual scan trigger, trading enable/disable, and kill switch controls.

## Default Parameters (from Pine Scripts)

These are the baseline defaults. All are configurable via TOML:

```toml
# Core
ema_fast = 20
ema_slow = 50
ema_trend = 200
atr_length = 14
rsi_length = 14
volume_ma_length = 20
adx_length = 14
adx_smoothing = 14
macd_fast = 12
macd_slow = 26
macd_signal = 9

# Session volume
session_volume_baseline_length = 34
session_volume_shock_z = 2.20
session_breakout_volume_ratio = 1.15

# Trend engine
min_directional_gap = 10
min_confidence_15m = 72
min_confidence_1h = 67
min_confidence_4h = 64
min_confidence_1d = 58
structure_lookback = 18
min_structure_score = 60

# Entry plan
swing_lookback = 24
ew1_min_atr = 0.12
ew1_max_atr = 0.85
ew2_atr = 0.42
ew3_atr = 0.78
deep_add_atr = 1.12
entry_zone_atr = 0.15
tp1_atr = 0.48
tp2_atr = 0.88
tp3_atr = 1.35
tp_step_min_atr = 0.22
max_tp1_atr = 0.95
max_tp2_atr = 1.45
max_tp3_atr = 2.10

# Protection
trap_score_threshold = 60
trap_volume_z = 2.0
wick_trap_atr = 0.70
sl_structure_lookback = 20
min_sl_distance_atr = 0.50
max_sl_distance_atr = 3.20
sl_extra_asia_atr = 0.15
sl_extra_europe_atr = 0.22
sl_extra_usa_atr = 0.30

# Exchange
[exchange]
platform = "paper"
testnet = true
rate_limit_per_second = 10

# Server
[server]
enabled = false
host = "127.0.0.1"
port = 8080
request_timeout_secs = 10
auth_token_env = "COINNESIA_API_TOKEN"

# Alerts
[alerts]
enabled = false
poll_interval_secs = 2
batch_size = 25
dedupe_ttl_secs = 300

[alerts.telegram]
enabled = false
bot_token_env = "TELEGRAM_BOT_TOKEN"
chat_id_env = "TELEGRAM_CHAT_ID"
api_base_url = "https://api.telegram.org"
parse_mode = "HTML"
disable_web_page_preview = true

# Database
[database]
enabled = false
url_env = "DATABASE_URL"
max_connections = 10
min_connections = 1
connect_timeout_secs = 5
migrate_on_start = false

# Cache
[cache]
enabled = false
url_env = "VALKEY_URL"
key_prefix = "coinnesia"
pool_size = 10
ttl_seconds = 300

# Runtime
[runtime]
scan_interval_secs = 60
shutdown_timeout_secs = 15
max_symbol_tasks = 128
health_stale_after_secs = 180

# Trading
[trading]
enabled = false
mode = "scan_only"
order_type = "limit"
use_oco = true
trailing_stop_after_tp1 = true
trailing_stop_atr = 0.50

[trading.scaling]
ew1_pct = 40
ew2_pct = 30
ew3_pct = 20
deep_add_pct = 10
tp1_close_pct = 50
tp2_close_pct = 30
tp3_close_pct = 20

# Portfolio
[portfolio]
total_capital_usdt = 10000.0
reserve_pct = 10
max_open_positions = 10
max_positions_per_asset = 4

[portfolio.allocation]
btc_pct = 30
altcoin_pct = 25
gold_pct = 15
forex_pct = 15
stocks_pct = 15

# Risk
[risk]
max_risk_per_trade_pct = 2.0
position_sizing_method = "fixed_pct"
min_risk_reward = 1.5
max_trades_per_day = 20
cooldown_after_loss_secs = 300

[risk.drawdown]
warning_pct = 3.0
caution_pct = 5.0
critical_pct = 8.0
max_account_drawdown_pct = 15.0

[risk.kill_switch]
enabled = true
close_positions_on_trigger = true
max_api_errors = 10
manual_restart_required = true

# Backtest
[backtest]
enabled = false
start_date = "2024-01-01"
end_date = "2025-01-01"
initial_capital = 10000.0
data_source = "api"

[backtest.fees]
maker_fee_pct = 0.02
taker_fee_pct = 0.04
slippage_model = "fixed"
slippage_bps = 5
```

## Testing Requirements

- Every indicator module must have unit tests that verify output against known TradingView values
- Strategy tests must cover: signal generation, trap blocking, regime gating, session filtering
- Asset profile tests must verify that BTC/Altcoin/Gold/Forex/IDX produce different signal characteristics given the same price data
- Integration tests must validate the full pipeline: data fetch → indicators → strategy → alert format
- Exchange trait tests must verify order placement, cancellation, and balance queries against mock/testnet
- Trading tests must verify position scaling (EW1→EW2→EW3), TP/SL execution, and trailing stop behavior
- Portfolio tests must verify allocation limits, exposure caps, and rebalancing triggers
- Risk tests must verify position sizing calculations, drawdown detection, and kill switch activation
- Backtest tests must verify that the same signal sequence produces identical results to a manual walkthrough
- Backtest must produce identical signals as live scanner given the same OHLCV data (engine parity)
- API tests must cover Axum routes, authentication, request validation, and error responses
- Storage tests must cover Postgres repositories and migrations, preferably with test containers or isolated test databases
- Cache tests must cover Valkey keys, TTLs, pub/sub behavior, locks, and deduplication semantics
- Runtime tests must cover graceful shutdown, worker supervision, startup reconciliation gates, and health/readiness state
- Use `assert_relative_eq!` (from `approx` crate) for floating-point comparisons with epsilon tolerance

## Common Pitfalls

1. **Don't use SMA for RSI** — must be RMA/Wilder's smoothing
2. **Don't hardcode prices** — all distances are ATR multiples
3. **Don't ignore sessions** — volume and volatility are session-dependent
4. **Don't treat all assets the same** — each has a distinct profile and weight table
5. **Don't skip trap guard** — it exists to prevent the most common failure mode (trapped entries)
6. **Don't make TP3 look guaranteed** — always mark optional in alerts
7. **Don't fetch proxy per symbol** — fetch once, share across the cycle
8. **Don't use `unwrap()` in production paths** — use `?` or handle gracefully
9. **Don't block the async runtime** — no synchronous I/O in async contexts
10. **Don't ignore the directional gap** — a signal needs both high confidence AND clear directional dominance
11. **Don't bypass the risk module** — every trade must pass risk validation before execution
12. **Don't duplicate engine code in backtester** — import the same modules; only sim exchange/metrics are backtest-specific
13. **Don't call exchange SDK directly** — always go through the `Exchange` trait interface
14. **Don't exceed position plan** — EW1+EW2+EW3+Deep must never exceed 100% of planned size
15. **Don't ignore the kill switch** — when triggered, all trading stops; require manual restart
16. **Don't block the hot path on Postgres** — persist asynchronously or through bounded workers unless the write is required for execution safety
17. **Don't store durable accounting only in Valkey** — Valkey is for hot state, locks, pub/sub, and cache
18. **Don't put business logic in Axum handlers** — call application services from handlers
19. **Don't start live trading before reconciliation** — exchange state, Postgres state, risk state, and kill switch state must be checked first
20. **Don't leave side effects unqueued** — Telegram delivery and non-critical persistence should not slow symbol scanning

## Crate Dependencies (Expected)

| Crate | Purpose |
|---|---|
| `tokio` | Async runtime |
| `futures` | Async utilities (join_all, stream) |
| `axum` | High-performance web API framework |
| `tower` + `tower-http` | HTTP middleware, tracing, CORS, timeouts |
| `sqlx` | Async Postgres access and migrations |
| `redis` or `fred` | Redis-compatible Valkey client |
| `tvdata-rs` | Primary unofficial TradingView data fetching adapter |
| `tail-fin-tradingview` | Optional TradingView live streaming/Pine/catalog tooling adapter when needed |
| `reqwest` | HTTP client for Binance market data, Twelve Data REST, and Telegram Bot API |
| `binance-sdk` | Official Binance SDK (spot features); wraps Binance REST for klines (Phase 1.5+) and will power signed trading endpoints in Phase 2. Never bypass the `Exchange` trait. |
| `ta` | Optional reference crate for standard indicators; current indicator hot path is implemented internally |
| `redis` | Redis-compatible Valkey client |
| `serde` + `toml` | Configuration deserialization |
| `anyhow` | Application error handling |
| `thiserror` | Custom error types |
| `tracing` + `tracing-subscriber` | Structured logging |
| `chrono` + `chrono-tz` | Timezone-aware timestamps (WIB) |
| `clap` | CLI argument parsing (subcommands: scan, trade, backtest) |
| `rust_decimal` | Precise decimal arithmetic for financial calculations |
| `uuid` | Stable IDs for signals, orders, jobs, and audit records |
| `csv` | CSV read/write for backtest data and reports |
| `serde_json` | JSON report output |
| `async-trait` | Async trait support (Exchange trait) |
| `approx` | Float comparison in tests |

## When Adding New Features

1. Check if the feature exists in the Pine Script references (`docs/TV_Pine_Scripts/`)
2. Determine which asset classes it applies to
3. Add the indicator/logic in its own module
4. Wire it into the confidence scorer with appropriate weight (per asset profile)
5. Add TOML config parameters with sensible defaults
6. Write unit tests against known values
7. If it affects runtime operation, decide what belongs in Postgres, Valkey, API DTOs, and background workers
8. Update `docs/requirements.md` if the feature changes the signal flow or production architecture

## When Adding API Endpoints

1. Add routes under `src/api/`
2. Use Axum extractors for auth, state, query, and JSON payloads
3. Keep handlers thin; delegate to `app/` services
4. Add request/response DTOs and tests
5. Ensure sensitive endpoints require authentication
6. Do not expose secrets or raw exchange credentials

## When Adding Persistence Or Cache

1. Durable state goes to Postgres via `src/storage/`
2. Ephemeral hot state, locks, pub/sub, and dedupe keys go to Valkey via `src/cache/`
3. Add migrations for schema changes
4. Include startup/reconciliation behavior if data affects live trading
5. Avoid database calls in tight per-symbol indicator loops

## When Adding a New Exchange

1. Create `src/exchange/<name>.rs`
2. Implement the `Exchange` trait (all methods)
3. Add the platform name to the config enum
4. Add API credentials to config structure
5. Test against the exchange's testnet/sandbox
6. Verify order types supported (market, limit, stop, OCO)
7. Document any platform-specific limitations (e.g., no OCO support)

## When Modifying Trading/Risk Logic

1. Ensure changes work in all three modes: live, paper, and backtest
2. Verify the risk module still correctly vetoes invalid trades
3. Run backtests before and after to compare metrics
4. Never bypass risk checks — if a trade should be allowed, adjust the limits in config
5. Test kill switch behavior after changes

## When Fixing Bugs

1. Identify which asset class is affected
2. Check the Pine Script reference for expected behavior
3. Verify the indicator calculation matches TradingView output (use RMA for RSI, etc.)
4. Check if the bug is in the indicator, the weight, or the threshold
5. Write a regression test before fixing
6. If the bug is in trading/risk, verify the fix in backtest mode first
