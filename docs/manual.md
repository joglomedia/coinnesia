# coinnesia Manual

This manual explains how to operate and extend the current Rust project foundation. It describes what the code can do today and the boundaries that future implementation should preserve.

## 1. Project State

The project is currently a compileable Rust skeleton for a multi-asset signal bot. The codebase includes:

- CLI command routing in `src/main.rs`.
- Config parsing in `src/config/`.
- Shared market/domain types in `src/lib.rs`.
- Initial indicator implementations in `src/indicators/`.
- Strategy output and EW/TP/SL calculators in `src/strategy/`.
- Exchange, data, alerts, scanner, trading, portfolio, risk, and backtest module boundaries.

The code does not yet perform live market scanning, live order execution, Telegram network sending, or historical candle replay.

## 2. Installation

Install Rust and Cargo, then build from the repository root:

```bash
cargo build
```

Run tests:

```bash
cargo test
```

The first build requires network access to download crates.

## 3. Command Reference

All commands use `config/default.toml` unless `--config` or `COINNESIA_CONFIG` is supplied.

### Validate Config

```bash
cargo run -- check-config
```

Loads the config and logs:

- number of configured symbols
- exchange platform
- trading mode

This is the quickest smoke test after editing TOML.

### Run One Scan Cycle

```bash
cargo run -- scan-once
```

Current behavior:

1. Clones the loaded config.
2. Spawns one async task per configured symbol.
3. Calls the shared `StrategyEngine`.
4. Returns placeholder signal results.

Because data adapters are not implemented yet, the strategy receives empty candle slices and returns `WAIT` for insufficient candles.

### Run Scanner

```bash
cargo run -- scan
```

Current behavior is a one-cycle placeholder. The future production version should run a loop with:

- proxy prefetch once per cycle
- rate-limited OHLCV fetching
- concurrent symbol scans
- alert dispatch
- optional paper/live trading path

### Run Backtest

```bash
cargo run -- backtest
```

Current behavior logs configured backtest state. The future version should replay candles through the same indicator, strategy, portfolio, and risk modules used by live scanning.

## 4. Configuration Manual

Main file: `config/default.toml`.

### Indicators

```toml
[indicators]
ema_fast = 20
ema_slow = 50
ema_trend = 200
atr_length = 14
rsi_length = 14
```

Rules:

- RSI must use RMA/Wilder smoothing.
- ATR is the base distance unit for EW, TP, SL, and trap thresholds.
- Indicator logic should stay inside `src/indicators/`.

### Strategy

```toml
[strategy]
min_directional_gap = 10.0
min_confidence_15m = 72.0
min_confidence_1h = 67.0
min_confidence_4h = 64.0
min_confidence_1d = 58.0
```

The target strategy requires both:

- confidence above the timeframe threshold
- directional gap above `min_directional_gap`

Signals must pass six layers:

1. Trend
2. Momentum
3. Volume
4. Entry trigger
5. Anti-trap
6. Regime/session

The current strategy engine has the result types and placeholder evaluation path, not the complete six-layer implementation.

### Entry Plan

```toml
[entry_plan]
ew1_min_atr = 0.12
ew2_atr = 0.42
ew3_atr = 0.78
deep_add_atr = 1.12
tp1_atr = 0.48
tp2_atr = 0.88
tp3_atr = 1.35
```

The current `EntryPlanCalculator` computes EW, TP, and SL values from an anchor price and ATR. TP3 is represented as `tp3_optional`.

### Session

Session times are in WIB / Asia/Jakarta:

- Asia: `06:00-14:00`
- Europe/London: `14:00-22:00`
- USA/New York: `19:00-03:00`
- IDX: `09:00-15:00`
- Forex rollover avoid: `04:55-06:10`

Session classification code exists in `src/strategy/session.rs`.

### Symbols

Each symbol block has:

```toml
[[symbols]]
symbol = "BTCUSDT"
asset_class = "btc"
exchange = "binance"
timeframes = ["15m", "1h", "4h", "1d"]
```

Supported `asset_class` values in code:

- `btc`
- `altcoin`
- `gold`
- `forex`
- `stocks_idx`
- `stocks_us`

### Trading Mode

```toml
[trading]
enabled = false
mode = "scan_only"
```

Keep `enabled = false` until the exchange, risk, position sizing, and order execution implementations are complete and tested.

### Server, Database, Cache, Runtime

The production target includes an Axum API server, Postgres, and Valkey. The config shape is already reserved:

```toml
[server]
enabled = false
host = "127.0.0.1"
port = 8080

[database]
enabled = false
url_env = "DATABASE_URL"

[cache]
enabled = false
url_env = "VALKEY_URL"

[runtime]
scan_interval_secs = 60
max_symbol_tasks = 128
```

Use Postgres for durable trading, portfolio, signal, alert, order, fill, balance, and backtest data. Use Valkey for hot ephemeral state, scan snapshots, deduplication, locks, and pub/sub.

## 5. Module Guide

### `src/config/`

Defines the TOML schema and loads config files through `AppConfig::from_file`.

`src/config/profiles.rs` contains default asset weight tables. These profiles encode the design philosophy:

- BTC: structure first
- Altcoin: anti-trap first
- Gold: proxy/session first
- Forex: session/RR first
- IDX stocks: volume/guard first

### `src/indicators/`

Contains one module per indicator system. Implementations should be deterministic for the same OHLCV input.

Implemented:

- `ema.rs`
- `atr.rs`
- `rsi.rs`
- `candle.rs`
- `volume.rs`
- `vwap.rs`
- partial `macd.rs`

Type placeholders exist for ADX/DMI, SMC, liquidity, order blocks, support/resistance, and regime.

### `src/strategy/`

Contains signal and plan logic.

Current implemented pieces:

- `SignalState`: `Long`, `Short`, `Wait`, `Freeze`
- `ConfidenceScore`
- `EntryPlanCalculator`
- `TakeProfits`
- `StopLoss`
- session classification
- trap decision type

The signal generator is intentionally conservative and currently returns `WAIT` for incomplete data or unimplemented layers.

### `src/data/`

Defines the `MarketDataSource` trait. Provider modules exist for:

- Binance
- TradingView
- Yahoo Finance
- proxy symbols

Current providers return empty candle vectors.

The target design requires proxy symbols such as XAUUSD and IHSG to be fetched once per scan cycle, then shared across symbols.

### `src/exchange/`

Defines the `Exchange` trait:

- `place_order`
- `cancel_order`
- `balances`

`paper.rs` accepts simulated orders and returns synthetic order IDs. Live exchange modules are placeholders.

All future trading code must use this trait instead of calling exchange SDKs directly.

### `src/risk/`

Contains risk boundaries and a simple `RiskManager` placeholder.

Production behavior should include:

- position sizing
- daily limits
- exposure limits
- drawdown state
- kill switch
- manual restart requirements

### `src/trading/`

Contains the trading engine shell, scaling plan, order status, and position type.

Important invariant: EW1 + EW2 + EW3 + deep add must not exceed 100% of the planned position. The default config has a test for this.

### `src/backtest/`

Contains the future backtest module layout. It must call the same indicator, strategy, risk, and portfolio code used by the scanner.

Backtest-specific code should only cover:

- historical data loading
- candle replay
- simulated exchange
- simulated portfolio
- metrics
- reports
- optimization

## 6. Indicator Development Rules

When adding or changing indicators:

1. Put each indicator in its own module.
2. Keep calculations deterministic.
3. Use `approx::assert_relative_eq!` in tests.
4. Add tests against TradingView/Pine reference values when available.
5. Do not hardcode asset-specific behavior in generic indicator modules.
6. Use RMA for RSI.
7. Use ATR multiples for distances.

## 7. Strategy Development Rules

When implementing the real signal engine:

1. Preserve `LONG`, `SHORT`, `WAIT`, `FREEZE` semantics.
2. Never generate a tradable signal during `FREEZE`.
3. Apply trap guard before returning a tradable signal.
4. Require both confidence threshold and directional gap.
5. Keep asset-specific differences in asset adapters or config profiles.
6. Mark TP3 optional in alerts.

## 8. Trading And Risk Rules

The required production flow is:

```text
Signal -> Risk Check -> Position Size -> Order -> Exchange
```

Do not bypass the risk layer.

Do not call Binance, MEXC, Bybit, OKX, or any exchange SDK from trading, portfolio, risk, or backtest modules. Use the `Exchange` trait.

When the kill switch triggers, trading should require manual review before restart.

## 9. Testing

Recommended baseline before any handoff:

```bash
cargo fmt
cargo test
cargo build
```

Current test coverage includes:

- default config parse
- EMA seeding/smoothing
- Wilder/RMA smoothing
- RSI RMA behavior
- ATR-based entry plan distances
- scaling total at 100%

Future test coverage should add:

- TradingView fixture parity for each indicator
- strategy layer tests
- trap blocking tests
- session filtering tests
- asset profile behavior tests
- exchange trait tests with mocks
- risk veto tests
- live/backtest engine parity tests

## 10. Implementation Roadmap

Suggested order:

1. Add Axum `serve` mode and a supervised runtime for 24/7 operation.
2. Add Postgres persistence and migrations.
3. Add Valkey cache/pub-sub/locks.
4. Implement market data adapters behind `MarketDataSource`.
5. Add candle fixture loading for deterministic tests.
6. Complete ADX/DMI, MACD parity, VWAP session anchoring, SMC, liquidity, and volume engines.
7. Implement trap guard and regime classifier.
8. Build confidence scoring from asset profile weights.
9. Wire scanner to fetch OHLCV and proxy snapshots.
10. Implement alert queue and Telegram delivery worker.
11. Complete paper trading path through risk and exchange traits.
12. Implement backtest replay using the same live engine.
13. Add live Binance implementation only after paper and backtest paths are tested.
