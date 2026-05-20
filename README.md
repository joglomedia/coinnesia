# coinnesia

`coinnesia` is a Rust 2021 multi-asset trading signal scanner. The project is designed to port TradingView Pine Script strategies into a fast async Rust engine that can scan crypto, gold tokens, forex, and equities, then route alerts or trades through shared strategy, risk, and exchange abstractions.

Current status: the repository contains the compileable project foundation. Core architecture, configuration loading, CLI entry points, exchange/data traits, asset profiles, and several deterministic indicator primitives are in place. Live market data, real exchange adapters, Telegram delivery, complete strategy scoring, and backtesting execution are still implementation work.

## Features In Place

- Tokio-based CLI binary with `check-config`, `scan-once`, `scan`, and `backtest` commands.
- TOML configuration loader backed by `config/default.toml`.
- Shared domain types for `AssetClass`, `Timeframe`, and OHLCV `Candle`.
- Modular crate layout matching the requirements document.
- Asset profile weight tables for BTC, altcoin, gold, forex, and IDX stocks.
- Indicator trait and initial indicator implementations:
  - EMA
  - ATR using Wilder/RMA smoothing
  - RSI using TradingView-style RMA smoothing
  - VWAP, volume ratio, MACD scaffold, candle shape helpers
- Strategy result types for `LONG`, `SHORT`, `WAIT`, and `FREEZE`.
- ATR-based entry window, TP, and SL calculators.
- Exchange trait plus a paper exchange stub.
- Scanner skeleton using `tokio::spawn` and `futures::join_all`.
- Unit tests for config parsing, EMA, RMA/ATR smoothing, RSI parity behavior, entry plan distances, and scaling totals.

## Important Documents

- [AGENTS.md](AGENTS.md) - project rules for LLM/code agents.
- [docs/requirements.md](docs/requirements.md) - full target architecture and strategy specification.
- [docs/indicators.md](docs/indicators.md) - indicator analysis and asset-specific interpretation.
- [docs/manual.md](docs/manual.md) - practical manual for setup, commands, config, and development.
- [docs/architecture_audit.md](docs/architecture_audit.md) - architecture review and target production stack.
- [docs/development_plan.md](docs/development_plan.md) - phased execution plan with milestones and acceptance gates.
- [docs/TV_Pine_Scripts/](docs/TV_Pine_Scripts) - Pine Script references.

Read `AGENTS.md` and `docs/requirements.md` before changing strategy, trading, risk, exchange, or backtest behavior.

## Requirements

- Rust toolchain compatible with Rust 2021.
- Cargo.
- Network access for the first `cargo build` or `cargo test`, so dependencies can be downloaded from crates.io.

The current code has no required API keys because live data sources and live exchange implementations are placeholders.

## Quick Start

```bash
cargo build
cargo test
cargo run -- check-config
```

Expected `check-config` behavior: the binary loads `config/default.toml` and logs the number of configured symbols, exchange platform, and trading mode.

Use a custom config path:

```bash
cargo run -- --config path/to/config.toml check-config
```

Or use the environment variable:

```bash
COINNESIA_CONFIG=path/to/config.toml cargo run -- check-config
```

## CLI Commands

```bash
cargo run -- check-config
```

Loads the TOML config and prints a concise summary through `tracing`.

```bash
cargo run -- scan-once
```

Runs one scanner cycle. Today this exercises the scanner and strategy skeleton. It does not fetch real OHLCV data yet, so configured symbols return `WAIT` with the current placeholder path.

```bash
cargo run -- scan
```

Runs the scanner placeholder. Current behavior is one scan cycle, not a persistent production loop.

```bash
cargo run -- backtest
```

Initializes the backtest placeholder and logs configured backtest dates. It does not replay historical data yet.

## Configuration

Default configuration lives in [config/default.toml](config/default.toml).

Major sections:

- `[indicators]` - EMA, ATR, RSI, ADX, MACD, and volume periods.
- `[strategy]` - confidence thresholds, directional gap, structure settings.
- `[entry_plan]` - ATR multiples for EW1/EW2/EW3, deep add, TP, and SL.
- `[trap_guard]` - trap score and wick/volume thresholds.
- `[session]` - WIB session definitions.
- `[exchange]` - selected platform and rate limit.
- `[trading]` - operating mode, order type, OCO/trailing flags, scaling plan.
- `[portfolio]` - capital, reserve, allocation, and position limits.
- `[risk]` - per-trade risk, drawdown levels, and kill switch settings.
- `[backtest]` - backtest dates, initial capital, fees, and slippage.
- `[[symbols]]` - scanner symbols and asset classes.
- `[proxy_symbols]` - XAUUSD/IHSG/DXY proxy tickers.

The default mode is `scan_only`, trading is disabled, and exchange platform is `paper`.

## Architecture

```text
src/
├── config/       TOML config structs, defaults, asset profile weights
├── data/         Market data source trait and provider placeholders
├── indicators/   Indicator trait and technical indicator modules
├── strategy/     Confidence, signals, session, trap guard, EW/TP/SL
├── assets/       Asset-specific adapters and philosophy markers
├── exchange/     Exchange trait and paper/live exchange placeholders
├── trading/      Execution, order, position, and scaling modules
├── portfolio/    Allocation, balance, exposure, rebalancing modules
├── risk/         Position sizing, drawdown, limits, kill switch modules
├── backtest/     Backtest engine and simulation module placeholders
├── alerts/       Alert sink trait and Telegram formatter placeholder
└── scanner/      Scan orchestration and rate limiter
```

## Safety Model

The target architecture is intentionally conservative:

- Signals can be `LONG`, `SHORT`, `WAIT`, or `FREEZE`.
- `FREEZE` means no trading should occur.
- Every trade must pass the risk module before exchange execution.
- Live trading must go through the `Exchange` trait.
- TP3 must be treated as optional in alert formatting.
- RSI must use RMA/Wilder smoothing, not SMA.
- All entry, TP, SL, and trap distances should be ATR-based.

The current repository enforces some of this structurally, but full production checks are still incomplete.

## Current Limitations

- Binance, TradingView, Yahoo Finance, MEXC, Bybit, and OKX modules are placeholders.
- Telegram sending is a stub; formatting exists but network delivery does not.
- Scanner does not fetch candles yet.
- Strategy scoring currently returns placeholder `WAIT` for normal paths.
- ADX/DMI, full MACD parity, SMC, liquidity sweep, support/resistance, trap guard, and regime classification need full implementations and fixtures.
- Backtest does not replay candles or produce metrics yet.
- Trading execution does not place real orders.

## Development Workflow

Run before handing off changes:

```bash
cargo fmt
cargo test
cargo build
```

For indicator changes, add fixture-backed tests and use `approx::assert_relative_eq!` for floating-point comparisons.

For trading/risk/exchange changes, preserve this flow:

```text
Signal -> Risk Check -> Position Size -> Order -> Exchange
```

For backtesting, import the same indicator, strategy, risk, and portfolio modules used by live scanning. Do not duplicate engine logic inside `backtest/`.
