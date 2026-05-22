# coinnesia

`coinnesia` is a Rust 2021 multi-asset trading signal scanner and trading-platform foundation. It ports TradingView Pine Script strategy ideas into an async Rust engine that can scan crypto, gold tokens, forex, and equities, persist/cached signal output, send Telegram alerts, and expose CLI plus Axum API controls.

Current status: Phase 0 and Phase 1 are implemented. The service runtime, Axum health/config/scan API, Postgres and Valkey foundations, Binance/TradingView/Yahoo market-data adapters, indicator suite, six-layer strategy scanner, scanner publishing pipeline, and Telegram alert worker are in place. Remaining major work is Phase 2+ live/paper trading, portfolio/risk execution integration, and event-driven backtesting.

## Features In Place

- CLI commands: `check-config`, `serve`, `migrate`, `scan-once`, `scan`, `trade`, and `backtest`.
- Axum API routes: `/health`, `/ready`, `/metrics`, `/config`, and authenticated `POST /scan`.
- Tokio service kernel with supervised scanner, alert, trading, and reconciliation workers.
- Optional Postgres pool, migrations, and repositories for signals, alerts, orders, fills, positions, balances, risk events, kill switch, backtests, and audit events.
- Optional Valkey cache with namespaced keys, TTL helpers, JSON helpers, locks, dedupe, pub/sub, rate-limit buckets, and heartbeats.
- Market data through the internal `MarketDataSource` trait:
  - Binance public HTTP klines.
  - TradingView via `tvdata-rs`.
  - Yahoo Finance chart fallback for daily+ data and proxies.
- Deterministic indicators: EMA, ATR/RMA, RSI/RMA, ADX/DMI, MACD, VWAP, volume, candle shape, SMC, liquidity, order block, support/resistance, and regime.
- Six-layer strategy pipeline with confidence scoring, timeframe thresholds, session gating, trap guard, regime blocking, and ATR-based EW/TP/SL.
- Scanner pipeline split into ingestion, analysis, and publishing with bounded symbol concurrency.
- Valkey scan/signal snapshots and Postgres signal evaluations.
- Queued Telegram alert jobs, Telegram Bot API sender, delivery attempt persistence, Valkey dedupe, and TP3 optional formatting.

## Important Documents

- [AGENTS.md](AGENTS.md) - project rules for LLM/code agents.
- [docs/requirements.md](docs/requirements.md) - full target architecture and strategy specification.
- [docs/indicators.md](docs/indicators.md) - indicator analysis and asset-specific interpretation.
- [docs/manual.md](docs/manual.md) - practical manual for setup, commands, config, and development.
- [docs/architecture_audit.md](docs/architecture_audit.md) - architecture review and remaining gaps.
- [docs/development_plan.md](docs/development_plan.md) - phased execution plan with status and acceptance gates.
- [docs/TV_Pine_Scripts/](docs/TV_Pine_Scripts) - Pine Script references.

## Quick Start

```bash
cargo build
cargo test -- --test-threads=1
cargo run -- check-config
cargo run -- scan-once
```

`scan-once` fetches configured candles, evaluates strategy, and returns one scan cycle. If `[database]` and `[cache]` are enabled, it also persists signal evaluations and caches latest scan/signal snapshots.

## Runtime Services

Use Podman/Docker Compose for Postgres and Valkey, then set:

```bash
DATABASE_URL=postgres://coinnesia:coinnesia@localhost:5432/coinnesia
VALKEY_URL=redis://localhost:6379/0
```

Enable services in `config/default.toml` or a custom config:

```toml
[database]
enabled = true

[cache]
enabled = true

[alerts]
enabled = true

[alerts.telegram]
enabled = true
```

Telegram credentials are read from environment variables:

```bash
TELEGRAM_BOT_TOKEN=...
TELEGRAM_CHAT_ID=...
```

## CLI Commands

```bash
cargo run -- check-config
cargo run -- migrate
cargo run -- scan-once
cargo run -- scan
cargo run -- serve
cargo run -- backtest
```

- `serve` starts Axum plus supervised workers.
- `scan` runs continuously at `runtime.scan_interval_secs`.
- `scan-once` runs one full ingestion, analysis, and publishing cycle.
- `migrate` applies Postgres migrations when database config is enabled.
- `trade` and full `backtest` are still Phase 2+ work.

## Configuration

Default configuration lives in [config/default.toml](config/default.toml).

Major sections:

- `[indicators]`, `[strategy]`, `[entry_plan]`, `[trap_guard]`, `[session]`
- `[server]`, `[alerts]`, `[database]`, `[cache]`, `[runtime]`
- `[data_sources]`, `[exchange]`, `[trading]`, `[portfolio]`, `[risk]`, `[backtest]`
- `[[symbols]]` and `[proxy_symbols]`

Secrets are env-driven. Do not commit Binance, TradingView, Telegram, Postgres, or Valkey credentials.

## Architecture

```text
src/
├── app/           service kernel, shutdown, supervisor, reconciliation gate
├── api/           Axum routes, auth, DTOs, metrics middleware
├── config/        TOML config structs, defaults, asset profile weights
├── data/          MarketDataSource trait and Binance/TradingView/Yahoo/proxy adapters
├── storage/       Postgres pool, migrations, records, repositories
├── cache/         Valkey keys, locks, pub/sub, rate limits, snapshots
├── indicators/    technical indicator modules
├── strategy/      confidence, signals, session, trap guard, EW/TP/SL
├── alerts/        Telegram formatter/sender and alert worker
├── scanner/       ingestion, analysis, publishing, continuous loop
├── exchange/      Exchange trait and paper/live exchange modules
├── trading/       execution, order, position, and scaling modules
├── portfolio/     allocation, balance, exposure, rebalancing modules
├── risk/          position sizing, drawdown, limits, kill switch modules
└── backtest/      backtest engine and simulation modules
```

## Current Limitations

- Live exchange trading execution is not complete.
- Portfolio/risk modules are present but not fully wired into an execution engine.
- Backtest does not yet replay historical data through full simulated fills and reports.
- API read endpoints for signals, orders, positions, portfolio, and risk are planned but not complete.
- Scanner performance target for 500+ symbols still needs benchmark validation.

## Development Workflow

Run before handing off changes:

```bash
cargo fmt
cargo test -- --test-threads=1
cargo run -- check-config
cargo run -- scan-once
```

For indicator changes, add fixture-backed tests and use `approx::assert_relative_eq!` for floating-point comparisons.

For trading/risk/exchange changes, preserve this flow:

```text
Signal -> Risk Check -> Position Size -> Order -> Exchange
```

For backtesting, import the same indicator, strategy, risk, and portfolio modules used by live scanning. Do not duplicate engine logic inside `backtest/`.
