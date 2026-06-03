# coinnesia — Operations Manual

This manual explains how to operate and extend the project. It describes current behavior of every
CLI command and HTTP API endpoint, data-source requirements, signal output format, and the rules
that future implementations must preserve.

---

## 1. Project State

**Phase 0–1** complete. **Phase 2+** (live trading, portfolio/risk expansion, backtester) pending.

The codebase performs real-time market-data scanning, six-layer signal evaluation, Valkey
snapshot caching, Postgres signal/alert persistence, and Telegram alert delivery when configured.
Order execution is not yet implemented.

Module layout:

| Path | Responsibility |
|---|---|
| `src/main.rs` | CLI entry point, command routing |
| `src/app/` | AppState bootstrap, Supervisor, shutdown, reconciliation |
| `src/api/` | Axum router, handlers, auth middleware, DTOs |
| `src/config/` | TOML schema, AppConfig loader, asset profiles |
| `src/data/` | MarketDataSource trait, Binance/TradingView/TwelveData adapters, retry, WS stream |
| `src/indicators/` | Deterministic indicator implementations (14 modules) |
| `src/strategy/` | Six-layer evaluation, confidence scoring, entry/TP/SL planning |
| `src/scanner/` | Ingestion, analysis, publishing pipeline |
| `src/alerts/` | Telegram alert worker, deduplication |
| `src/storage/` | Postgres repositories, migrations |
| `src/cache/` | Valkey helpers, snapshots, pub/sub, locks |
| `src/exchange/` | Exchange trait, paper adapter, binance scaffold |
| `src/trading/` | Trading engine shell (Phase 2) |
| `src/portfolio/` | Portfolio manager shell (Phase 3) |
| `src/risk/` | Risk manager shell (Phase 3) |
| `src/backtest/` | Backtester shell (Phase 4) |

---

## 2. Installation

```bash
# Install Rust (1.86+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build
cargo build

# Test
cargo test
```

The first build downloads ~150 crates including `binance-sdk`, `tokio`, `axum`, and `sqlx`.

### Required environment variables

Copy `.env.example` to `.env` and fill in values before running:

```bash
cp .env.example .env
```

| Variable | Required for | Notes |
|---|---|---|
| `DATABASE_URL` | DB-enabled commands | `postgres://user:pass@host:5432/db` |
| `VALKEY_URL` | Cache-enabled commands | `redis://localhost:6379/0` |
| `BINANCE_API_KEY` | Phase 2 (trading) | Not needed for public market data |
| `BINANCE_API_SECRET` | Phase 2 (trading) | Not needed for public market data |
| `TRADINGVIEW_SESSION_ID` | Proxy symbols via TV | Browser cookie `sessionid` from `tradingview.com` |
| `TRADINGVIEW_SESSIONID_SIGN` | Proxy symbols via TV | Browser cookie `sessionid_sign` from `tradingview.com` |
| `TRADINGVIEW_AUTH_TOKEN` | Optional | JWT from TV WebSocket handshake — omit if session is set |
| `TELEGRAM_BOT_TOKEN` | Alert delivery | From `@BotFather` |
| `TELEGRAM_CHAT_ID` | Alert delivery | Target chat/channel ID |
| `COINNESIA_API_TOKEN` | `POST /scan` auth | Free-form secret string |

---

## 3. CLI Command Reference

All commands load `config/default.toml` unless overridden:

```bash
cargo run -- --config path/to/config.toml <command>
# or via environment variable:
COINNESIA_CONFIG=path/to/config.toml cargo run -- <command>
```

---

### `check-config`

**Purpose:** Validate configuration and print a summary. Use after every TOML edit.

```bash
cargo run -- check-config
```

**What it does:**

1. Calls `AppConfig::from_file()` — parses and validates TOML
2. Logs: symbol count, exchange platform, trading mode, server/database/cache enabled flags

**Expected output:**
```
INFO coinnesia: configuration loaded
  symbols=3 exchange=paper trading_mode=scan_only
  server_enabled=false database_enabled=true cache_enabled=true
```

**Fails if:** TOML is malformed or required fields are missing.

---

### `scan-once`

**Purpose:** Execute one complete scan cycle and exit. The primary debugging and validation command.

```bash
cargo run -- scan-once
# With verbose signal output (default since Phase 1.5):
RUST_LOG=info cargo run -- scan-once
```

#### Full 7-Stage Workflow

```
┌─────────────────────────────────────────────────────────────────────┐
│ Stage 1 — Proxy Fetch (non-blocking)                                 │
│ Source: proxy_symbols.*.source (tradingview or twelvedata)           │
│ Symbols: OANDA:XAUUSD, IDX:COMPOSITE, TVC:DXY  →  ProxySnapshot    │
│ Failure: .unwrap_or_default() → empty snapshot, scan continues      │
└──────────────────────────────┬──────────────────────────────────────┘
                               ↓
┌─────────────────────────────────────────────────────────────────────┐
│ Stage 2 — OHLCV Batch Fetch                                          │
│ Source: PerSymbolMarketData (routes per symbol config)               │
│ Candles: candle_limit (default 250) bars at primary timeframe        │
│ BTCUSDT  → Binance REST  → 250 × M15 bars (~2.6 days)              │
│ ETHUSDT  → Binance REST  → 250 × M15 bars                          │
│ PAXGUSDT → Binance REST  → 250 × H1  bars (~10 days)               │
│ Runs concurrently per source group via tokio::join!                  │
└──────────────────────────────┬──────────────────────────────────────┘
                               ↓
┌─────────────────────────────────────────────────────────────────────┐
│ Stage 3 — Indicator Calculation  (per symbol, concurrent)            │
│ For each ScanWorkItem {symbol, timeframe, candles}:                  │
│   EMA 20/50/200 · ATR/RMA · RSI(RMA) · ADX/DMI · MACD              │
│   VWAP (anchored + daily WIB) · Volume engine · Candle shape        │
│   SMC (BOS/CHOCH) · Liquidity sweeps · Order blocks                 │
│   Support/Resistance zones · Market regime                          │
└──────────────────────────────┬──────────────────────────────────────┘
                               ↓
┌─────────────────────────────────────────────────────────────────────┐
│ Stage 4 — Six-Layer Evaluation  (per symbol)                         │
│ Layer 1: Regime gate                                                 │
│   Shock → SignalState::Freeze  (no further evaluation)              │
│   Sideways/Chop → SignalState::Wait with reason regime_block:*      │
│   Trend/Expansion → continue                                        │
│ Layer 2: Session gate (WIB / Asia/Jakarta UTC+7)                    │
│   session_allows_asset(session, asset_class) → else Wait            │
│ Layer 3: Confidence scoring                                         │
│   evaluate_direction(Long) and evaluate_direction(Short)            │
│   Returns score (0–100) per side from asset-profile weights          │
│ Layer 4: MTF threshold check                                        │
│   threshold_for_timeframe(M15=72, H1=67, H4=64, D1=58)             │
│   score < threshold → Wait with reason layers_not_met               │
│ Layer 5: Trap guard                                                  │
│   Wick chaos + volume shock + cooldown check                        │
│   Both sides blocked → Wait with reason trap_guard_blocked          │
│ Layer 6: Directional gap check                                      │
│   |long_score - short_score| < min_directional_gap → Wait          │
│   Winner side → Long or Short                                       │
└──────────────────────────────┬──────────────────────────────────────┘
                               ↓
┌─────────────────────────────────────────────────────────────────────┐
│ Stage 5 — Entry Plan Calculation  (only if Long or Short)            │
│ ATR-based bands around anchor = latest close price                   │
│ EW1 (40%)  EW2 (30%)  EW3 (20%)  Deep Add (10%)                    │
│ TP1 / TP2 / TP3_optional  +  Stop Loss                              │
└──────────────────────────────┬──────────────────────────────────────┘
                               ↓
┌─────────────────────────────────────────────────────────────────────┐
│ Stage 6 — Publish                                                    │
│ No DB/cache:  no-op                                                  │
│ Cache only:   ScanSnapshot + SignalSnapshot → Valkey (TTL 300s)     │
│ DB enabled:   signal_evaluations table + alert_jobs table           │
│ Both:         Valkey pub/sub channel "signals" (fan-out to workers) │
└──────────────────────────────┬──────────────────────────────────────┘
                               ↓
┌─────────────────────────────────────────────────────────────────────┐
│ Stage 7 — Log Summary                                                │
│ INFO scan cycle completed  cycle_id=… scanned=N signals=N           │
│ INFO signal  symbol=…  state=…  long=…  short=…  reason=…          │
│  (one signal line per symbol, regardless of state)                   │
└─────────────────────────────────────────────────────────────────────┘
```

#### Reading the Signal Output

```
INFO signal symbol=BTCUSDT state=Wait long=0.0 short=55.0 gap=55.0
             reason=layers_not_met threshold=72.0 long=trend|momentum|volume|entry short=trend|volume
             entry_plan=—

INFO signal symbol=ETHUSDT state=Wait long=0.0 short=0.0 gap=0.0
             reason=regime_block:Sideways
             entry_plan=—

INFO signal symbol=PAXGUSDT state=Wait long=0.0 short=0.0 gap=0.0
             reason=regime_block:Sideways
             entry_plan=—
```

| Field | Meaning |
|---|---|
| `state` | `Long` `Short` `Wait` `Freeze` — see Signal State table below |
| `long` | Long confidence score 0–100 |
| `short` | Short confidence score 0–100 |
| `gap` | `|long − short|` — directional clarity |
| `reason` | Which layer caused the result — see Reason Codes below |
| `entry_plan` | `ew1=low~high tp1=… tp2=… tp3=… sl=…` or `—` if Wait/Freeze |

**Signal state semantics:**

| State | Meaning | Entry plan? | Telegram alert? |
|---|---|---|---|
| `Long` | All 6 layers pass, buy side wins | Yes | Yes |
| `Short` | All 6 layers pass, sell side wins | Yes | Yes |
| `Wait` | One or more layers not met | No | No |
| `Freeze` | Shock regime — all trading halted | No | No |

**Reason codes:**

| Reason | Layer blocked | Description |
|---|---|---|
| `not_enough_candles` | Pre-check | Fewer candles than `atr_length + 1` |
| `market_data_unavailable` | Pre-check | Empty candle response from data source |
| `symbol_not_configured` | Pre-check | Symbol absent from `[[symbols]]` config |
| `shock_regime` | Layer 1 | ATR expansion + volume shock → Freeze |
| `regime_block:Sideways` | Layer 1 | ADX weak, no directional expansion |
| `regime_block:Chop` | Layer 1 | Mixed oscillation |
| `session_block:None` | Layer 2 | Outside all active sessions |
| `session_block:Asia` | Layer 2 | Asia session but asset not allowed (e.g., forex) |
| `trap_guard_blocked` | Layer 5 | Both Long and Short blocked by wick/volume trap |
| `layers_not_met threshold=72.0 long=… short=…` | Layer 3–6 | Score or gap below threshold |
| `six_layer_pass timeframe=M15 session=Europe` | — | **Actionable signal** |

**When to act:** Only `state=Long` or `state=Short` with `reason=six_layer_pass` is a tradable signal.
`signals=3` in the summary does not mean 3 trade signals — it means 3 evaluations ran (one per symbol).

---

### `scan`

**Purpose:** Continuous scanner loop. Runs `scan-once` repeatedly at `runtime.scan_interval_secs`
(default 60 seconds). Use for 24/7 standalone scanning without the full service stack.

```bash
cargo run -- scan
```

**What it does:**

1. On each tick: executes the full 7-stage scan workflow
2. Logs `scanner loop completed one cycle` after each cycle
3. Runs until killed (SIGINT/SIGTERM not handled in this mode — use `serve` for graceful shutdown)

**Limitations vs `serve`:**
- No Axum API
- No worker health monitoring
- No graceful shutdown handling
- No streaming mode (polling only)

For production use, prefer `serve`.

---

### `serve`

**Purpose:** Full 24/7 service mode. Starts the Axum HTTP API and all four supervised background workers.

```bash
cargo run -- serve
```

**What it does:**

```
AppState::bootstrap(config)
├── HealthRegistry        ← per-component health + heartbeat tracking
├── RuntimeMetrics        ← scan/signal/API/error counters
├── StartupGate           ← live-trading prerequisite check
├── Db::connect_optional  ← Postgres pool (if database.enabled = true)
└── Cache::connect_optional ← Valkey pool (if cache.enabled = true)

TcpListener → Axum router (5 routes)
  + metrics middleware (records latency + request count)

Supervisor::run() → JoinSet (4 workers, all cancellable via CancellationToken)
├── Scanner worker        ← continuous scan_once or streaming (scanning_mode)
├── Alert worker          ← polls alert_jobs, sends Telegram, deduplicates
├── Trading worker        ← placeholder (Phase 2)
└── Reconciliation worker ← startup-gate enforcement
```

**Workers:**

| Worker | Behavior | Config |
|---|---|---|
| Scanner | `scan_once` every `scan_interval_secs` OR event-driven via Binance WS | `runtime.scan_interval_secs`, `data_sources.scanning_mode` |
| Alert | Polls `alert_jobs` every `alerts.poll_interval_secs`, sends Telegram, records delivery | `alerts.*`, `alerts.telegram.*` |
| Trading | Logs placeholder; no execution yet | `trading.enabled` (keep `false`) |
| Reconciliation | Enforces startup gate (DB + cache + kill-switch required for live trading) | Internal |

**Graceful shutdown:** SIGINT or SIGTERM triggers `CancellationToken` propagation to all workers.
Default drain timeout: `runtime.shutdown_timeout_secs` (15s).

---

### `migrate`

**Purpose:** Run Postgres schema migrations. Must be run before using any DB-enabled feature.

```bash
cargo run -- migrate
```

**What it does:**

1. Connects to `DATABASE_URL`
2. Runs embedded SQL migrations from `src/storage/migrations/`
3. Current migration: `20260520000000_phase_0_4_foundation.sql`

**Tables created:**

`symbols`, `signal_evaluations`, `alert_jobs`, `alert_deliveries`, `orders`, `order_events`,
`fills`, `positions`, `balances`, `portfolio_snapshots`, `risk_events`, `backtest_runs`

**Run order:** Always run `migrate` before `serve` or `scan` when the database is enabled.
Alternatively, set `database.migrate_on_start = true` in config (not recommended for production).

```bash
# Via Docker Compose (recommended):
docker compose up migrator
# or manually:
cargo run -- migrate
```

---

### `trade`

**Status: placeholder — Phase 2.**

```bash
cargo run -- trade [--paper]
```

Currently logs `"trade command scaffolded; trading service wiring pending"` and exits.
Implementation target: Phase 2 execution service with order lifecycle, fills, and reconciliation.

---

### `backtest`

**Status: scaffold — Phase 4.**

```bash
cargo run -- backtest
```

Currently calls `BacktestEngine::new(config).run()` which is a minimal scaffold.
Implementation target: event-driven candle replay through the same indicator/strategy/risk pipeline
used by `scan-once`.

---

## 4. HTTP API Reference

Enable the API in config:

```toml
[server]
enabled = true
host    = "127.0.0.1"
port    = 8080
auth_token_env = "COINNESIA_API_TOKEN"  # env var name holding the token
```

Set the token:
```bash
export COINNESIA_API_TOKEN=my-secret-token
```

Start with `cargo run -- serve`. All responses are JSON unless noted.

---

### `GET /health`

Returns per-component health status. Does **not** require authentication.

**Response: 200 OK**
```json
{
  "healthy": true,
  "components": [
    {
      "name": "api",
      "healthy": true,
      "stale": false,
      "heartbeat_required": false,
      "last_heartbeat_age_secs": null
    },
    {
      "name": "scanner",
      "healthy": true,
      "stale": false,
      "heartbeat_required": true,
      "last_heartbeat_age_secs": 42
    }
  ],
  "reconciliation": {
    "database_ready": true,
    "cache_ready": true,
    "kill_switch_clear": true,
    "live_trading_allowed": false
  }
}
```

Components tracked: `api`, `supervisor`, `scanner`, `alert`, `trading`, `reconciliation`.

A component is `stale` when its last heartbeat exceeds `runtime.health_stale_after_secs` (180s).

---

### `GET /ready`

Kubernetes-style readiness probe. Returns **503** when any component is unhealthy or stale.
Does not require authentication.

**200 OK** → service ready to receive traffic  
**503 Service Unavailable** → one or more components unhealthy

Same response body format as `/health`.

---

### `GET /metrics`

Prometheus-compatible text metrics. Does not require authentication.

**Response: 200 OK (text/plain)**
```
coinnesia_up 1
coinnesia_scan_cycles_total 14
coinnesia_symbols_scanned_total 42
coinnesia_signals_generated_total 42
coinnesia_api_requests_total 7
coinnesia_exchange_errors_total 0
coinnesia_alert_send_failures_total 0
```

Suitable for scraping by Prometheus, Grafana Agent, or any Prometheus-compatible collector.

---

### `GET /config`

Returns sanitized configuration summary. Secrets (API keys, tokens, passwords) are **never** exposed.
Does not require authentication.

**Response: 200 OK**
```json
{
  "symbols": 3,
  "exchange": "paper",
  "trading_mode": "scan_only",
  "server_enabled": true,
  "database_enabled": true,
  "cache_enabled": true
}
```

---

### `POST /scan`

Triggers one scan cycle synchronously. **Requires Bearer token authentication.**

```bash
curl -X POST http://localhost:8080/scan \
  -H "Authorization: Bearer my-secret-token"
```

**Auth header format:** `Authorization: Bearer <token>` (value must match `COINNESIA_API_TOKEN`).
Missing or wrong token → **401 Unauthorized**.

**Response: 200 OK**
```json
{
  "accepted": true,
  "cycle_id": "40cd6f02-7f31-419b-aaa5-9b6839d0ce56",
  "scanned": 3,
  "signals": 3
}
```

`signals` is always equal to `scanned` — it is the count of evaluations, not actionable trade
signals. Signal detail (state, confidence, entry_plan) is logged server-side but not yet returned
in the response body (Phase 2 read APIs will expose per-signal data).

**Response: 502 Bad Gateway** (scan failed, e.g., data source unreachable)
```json
{
  "accepted": false,
  "reason": "scan failed: ..."
}
```

---

## 5. Signal Output Reference

### SignalResult fields

```rust
pub struct SignalResult {
    pub symbol:      String,          // e.g. "BTCUSDT"
    pub state:       SignalState,     // Long | Short | Wait | Freeze
    pub confidence:  ConfidenceScore, // { long, short, directional_gap }
    pub reason:      String,          // human-readable evaluation result
    pub entry_plan:  Option<EntryPlan>, // Some only when Long or Short
}
```

### EntryPlan fields (when state = Long or Short)

```
EW1  (Entry Wave 1, 40% of position size)
  low  = anchor − ew1_min_atr × ATR
  high = anchor + entry_zone_atr × ATR

EW2  (Entry Wave 2, 30%)
  low  = anchor − ew2_atr × ATR
  high = EW1.low

EW3  (Entry Wave 3, 20%)
  low  = anchor − ew3_atr × ATR
  high = EW2.low

Deep Add (10%)
  based on deep_add_atr multiplier

TP1  → close 50% of position (tp1_atr × ATR above anchor for Long)
TP2  → close 30% of remaining
TP3  → close 20% of remaining (optional, marked in alerts)
SL   → stop loss (sl_atr × ATR below anchor for Long; session adjustment adds extra buffer)
```

All distances are multiples of `ATR` (Wilder's 14-period smoothing) at the primary timeframe.
Config reference: `[entry_plan]` section in `config/default.toml`.

### Confidence scoring

Each side (long/short) accumulates a score 0–100 from asset-profile-weighted indicator layers:

| Layer | Key indicators |
|---|---|
| Trend | EMA alignment, ADX/DMI direction |
| Momentum | RSI zone, MACD histogram direction |
| Volume | Volume ratio vs MA, VolumePressure |
| Entry trigger | SMC structure (BOS/CHOCH), candle confirmation |
| Anti-trap | TrapGuard: wick-to-body ratio, volume shock Z-score |
| Regime/session | MarketRegime ≠ Shock/Sideways; session permits asset class |

Threshold required per timeframe (config: `[strategy]`):

| Timeframe | `min_confidence` | Typical `min_directional_gap` |
|---|---|---|
| M15 | 72.0 | 10.0 |
| H1 | 67.0 | 10.0 |
| H4 | 64.0 | 10.0 |
| D1 | 58.0 | 10.0 |

---

## 6. Configuration Manual

Main file: `config/default.toml`. All sections are documented inline.
Override a single value without copying the file:

```bash
COINNESIA_CONFIG=config/my_override.toml cargo run -- scan-once
```

### Key config sections

```toml
[indicators]        # indicator periods (EMA, ATR, RSI, ADX, MACD lengths)
[strategy]          # confidence thresholds and directional gap
[entry_plan]        # ATR multiples for EW1/2/3, TP1/2/3, SL
[trap_guard]        # wick/volume trap detection thresholds
[session]           # WIB session windows (Asia/Europe/USA/IDX)
[server]            # Axum host/port/auth_token_env
[alerts]            # alert worker enabled, poll interval, Telegram config
[database]          # Postgres pool settings, migrate_on_start
[cache]             # Valkey pool, key prefix, default TTL
[runtime]           # scan_interval_secs, max_symbol_tasks, health_stale_after_secs
[data_sources]      # primary, fallback, candle_limit, scanning_mode, retry
[exchange]          # platform, testnet, rate_limit_per_second, binance sub-config
[trading]           # enabled (keep false until Phase 2), mode, scaling
[portfolio]         # total capital, allocation %, max positions
[risk]              # risk per trade %, drawdown limits, kill switch
[backtest]          # date range, initial capital, fees model
[[symbols]]         # one entry per trading symbol
[proxy_symbols.*]   # XAUUSD, IHSG, DXY context symbols
[assets.altcoin]    # V62 altcoin overrides (EW/TP compress, trap sensitivity, profile)
[assets.gold]       # Gold V1 overrides (session bias mode, news window, proxy alignment)
[assets.forex]      # Forex V58 overrides (per-session RR, HTF counter-block)
[assets.stocks_idx] # IDX V5 overrides (RVOL min, value-traded floor, downside risk)
```

### V61.x parity knobs (sub-phase 1.7.12)

The following knobs were added alongside the V61.x Pine parity work. All have
defaults that mirror the Pine inputs, so an existing `config/default.toml` keeps
working unchanged — these are additive.

```toml
[indicators]
session_volume_baseline_length = 34   # rolling window for session volume MA
session_volume_shock_z = 2.20         # z-score above which a bar is "shock volume"
session_breakout_volume_ratio = 1.15  # current/baseline ratio gate for breakouts
bos_close_buffer_atr = 0.10           # BOS close confirmation buffer (ATR)
choch_close_buffer_atr = 0.12         # ChoCh close confirmation buffer (ATR)
liquidity_equal_atr = 0.20            # equal highs/lows tolerance (ATR)
ob_validation_vol_ratio = 1.10        # order-block volume confirmation gate
momentum_decay_bars = 8               # bars after which momentum decays to 0

[strategy]
min_structure_edge = 0.15             # min normalized long−short structural edge

[entry_plan]
ew_micro_1_atr = 0.06                 # tightest entry-window buffer (ATR)
ew_micro_2_atr = 0.12                 # mid entry-window buffer (ATR)
ew_micro_3_atr = 0.20                 # widest entry-window buffer (ATR)
ew_session_open_buffer_atr = 0.18     # extra buffer at session boundaries (ATR)
min_rr_trade = 1.6                    # minimum TP1/SL risk-reward to emit

[assets.altcoin]
alt_ew_vol_compress = 0.85            # EW compression for thin alts
alt_tp_thin_compress = 0.78           # TP compression for thin alts
alt_sl_wick_buffer_atr = 0.35         # extra SL buffer over wicks
alt_trap_sensitivity = 1.10           # trap-guard sensitivity multiplier
alt_min_break_body_atr = 0.55         # min breakout candle body (ATR)
alt_max_chase_atr = 0.60              # max chase distance beyond ideal entry
alt_ltf_weight = 1.20                 # LTF consensus weight multiplier
alt_htf_relax = 0.85                  # HTF bias relaxation
alt_profile = "AUTO"                  # AUTO | MAJOR | MID | MEME

[assets.gold]
gold_session_bias_mode = "hybrid"     # session | proxy | hybrid
gold_news_window_atr = 1.80           # extra ATR allowance during news windows
gold_proxy_min_alignment = 0.65       # 0..1 XAU proxy alignment required

[assets.forex]
forex_rr_asia = 1.40
forex_rr_europe = 1.80
forex_rr_usa = 2.00
forex_block_counter_htf = true        # block LTF signal that opposes HTF bias

[assets.stocks_idx]
idx_rvol_min = 1.10                   # min RVOL to qualify a setup
idx_cmf_length = 20                   # CMF length used by the IDX evaluator
idx_obv_slope_bars = 10               # OBV slope window
idx_value_traded_min = 500000000.0    # min value-traded per bar (IDR)
idx_rs_min = 0.0                      # min RS vs IHSG (≤0 disables the gate)
idx_downside_risk_threshold = 1.35    # downside-risk downgrade threshold (ATR)
```

Per-asset overrides are loaded with `serde(default)` — an entirely missing
`[assets]` section falls back to the Pine baseline.

### Data source configuration

**Binance (default primary — crypto symbols):**

```toml
[data_sources]
primary = "binance"
fallback = "tradingview"

[[symbols]]
symbol = "BTCUSDT"
data_source = "binance"   # explicit routing
timeframes = ["15m", "1h", "4h", "1d"]
```

**TradingView (proxy symbols, forex, indices):**

```toml
[data_sources.tradingview]
enabled = true
session_id_env = "TRADINGVIEW_SESSION_ID"
session_signature_env = "TRADINGVIEW_SESSIONID_SIGN"
# auth_token_env — OPTIONAL when session_id is set (omit or leave unset)
```

#### TradingView Session Setup (recommended — long-term solution)

Session cookies expire every 2–4 weeks. Refresh them with the following steps whenever you see
`WARN retrying after transient error` for proxy symbols in the scan log.

**Step 1 — Log in to TradingView**

Open `https://www.tradingview.com` in your browser and make sure you are logged in with an
account that has access to the data you need (free accounts work for basic OHLCV history).

**Step 2 — Open DevTools and navigate to the cookies panel**

| Browser | Shortcut |
|---|---|
| Chrome / Edge | `F12` → **Application** tab → **Storage** → **Cookies** → `https://www.tradingview.com` |
| Firefox | `F12` → **Storage** tab → **Cookies** → `https://www.tradingview.com` |
| Safari | Enable DevTools first: **Preferences → Advanced → Show Develop menu** → `Develop → Show Web Inspector` → **Storage** → **Cookies** |

**Step 3 — Copy the two required cookies**

Look for these exact cookie names in the list:

| Cookie name | Maps to env var | Notes |
|---|---|---|
| `sessionid` | `TRADINGVIEW_SESSION_ID` | 32-character alphanumeric string |
| `sessionid_sign` | `TRADINGVIEW_SESSIONID_SIGN` | `v3:` prefix followed by a base64 string |

Click each cookie row and copy the **Value** column.

**Step 4 — Update `.env`**

```bash
TRADINGVIEW_SESSION_ID="d7ncszfp7642vf3j2cnco3xcsz3v4zay"
TRADINGVIEW_SESSIONID_SIGN="v3:y9ksxkCC5lSD3z7/IGT8YEPC+aBFysQ3eh7poa/+pn0="
```

Do **not** include quotes from the browser's copy — paste the raw value only.

**Step 5 — Verify**

```bash
cargo run -- scan-once
```

A successful refresh produces no `WARN retrying` lines for proxy symbols. The scan output
should show only the `INFO scan cycle completed` and per-signal lines.

---

#### TradingView Auth Token (optional — advanced)

`TRADINGVIEW_AUTH_TOKEN` is the JWT sent in the WebSocket `set_auth_token` message during the
TradingView data protocol handshake. It is **separate from the session cookie** and is
**not required** when `TRADINGVIEW_SESSION_ID` is set — the library falls back to the literal
string `"unauthorized_user_token"` which is the standard value for session-authenticated connections.

Only set this if you need it explicitly (e.g., certain premium data feeds require a user JWT).

> **Important:** A chart-share JWT (identifiable by `"iss": "tv_chart"` and `"layoutId"` fields
> in its payload) is **not** a valid user auth token. Do not use chart share tokens for this field.

**How to obtain the correct user session JWT:**

1. Open `https://www.tradingview.com/chart/` in your browser.

2. Open DevTools (`F12`) → **Network** tab → filter by **WS** (WebSocket connections only).

3. In the connections list, click the WebSocket connection to `data.tradingview.com`.

4. Open the **Messages** sub-tab (Chrome) or **Response** (Firefox).

5. Scroll through the early frames until you find one containing `"set_auth_token"`:
   ```json
   {"m":"set_auth_token","p":["eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9..."]}
   ```

6. Copy the JWT string from inside the `"p"` array (the value after the first `["`).

7. Add it to `.env`:
   ```bash
   TRADINGVIEW_AUTH_TOKEN="eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9..."
   ```

The user-session JWT differs from a chart-share token in its payload:
- **User JWT**: contains `userId` or `sub`, no `layoutId`
- **Chart JWT**: contains `iss: "tv_chart"`, `layoutId`, `ownerId` — this is wrong for auth token

**Verification:** Decode any JWT at `jwt.io` to inspect its payload before using it.

---

#### Twelve Data Setup

Twelve Data provides a reliable REST API for proxy symbols (XAUUSD, DXY, IHSG) without
requiring a browser session. It works with a static API key that does not expire.

**Step 1 — Obtain a free API key**

Register at `https://twelvedata.com/pricing` → select the free plan → copy the API key from
your dashboard. Free tier: 800 API credits/day.

**Step 2 — Add the key to `.env`**

```bash
TWELVE_DATA_API_KEY=your_api_key_here
```

**Step 3 — Enable the adapter in config**

```toml
[data_sources.twelvedata]
enabled     = true
base_url    = "https://api.twelvedata.com"
api_key_env = "TWELVE_DATA_API_KEY"
```

**Step 4 — Verify symbols before switching**

Before setting `source = "twelvedata"` for any proxy symbol, confirm the symbol works with
your API key:

```bash
# Gold spot
curl "https://api.twelvedata.com/time_series?symbol=XAU/USD&interval=1day&outputsize=3&apikey=YOUR_KEY"

# US Dollar Index
curl "https://api.twelvedata.com/time_series?symbol=DXY&interval=1day&outputsize=3&apikey=YOUR_KEY"

# Jakarta Composite (IHSG) — verify this symbol is accessible on your plan
curl "https://api.twelvedata.com/symbol_search?symbol=COMPOSITE&apikey=YOUR_KEY"
```

A successful response has `"status": "ok"` with a non-empty `"values"` array.
An error response has `"status": "error"` with a `"message"` field.

**Step 5 — Switch proxy symbols to Twelve Data**

After confirming each symbol works, update `config/default.toml`:

```toml
[proxy_symbols.xauusd]
tradingview = "OANDA:XAUUSD"
twelvedata  = "XAU/USD"         # confirmed working
source      = "twelvedata"      # changed from "tradingview"

[proxy_symbols.dxy]
tradingview = "TVC:DXY"
twelvedata  = "DXY"             # verify first
source      = "twelvedata"

[proxy_symbols.ihsg]
tradingview = "IDX:COMPOSITE"
twelvedata  = "COMPOSITE"       # verify first
source      = "tradingview"     # keep TV until twelvedata symbol is confirmed
```

**Step 6 — Verify with scan-once**

```bash
cargo run -- scan-once
```

No `WARN retrying` lines for proxy symbols indicates successful fetches.

**Rate limit guidance (free tier)**

| Scenario | Req/day | Within free tier? |
|---|---|---|
| 3 proxies × 60s interval (default) | ~4320 | No (800 limit) |
| 3 proxies × 300s interval | ~864 | Yes (marginal) |
| 3 proxies × 600s interval | ~432 | Yes (comfortable) |

To adjust scan interval:
```toml
[runtime]
scan_interval_secs = 300   # 5 minutes — recommended for free tier
```

Paid plans (Starter $8/mo: 160 req/min, ~230k req/day) remove this constraint entirely.

**Proxy symbols configuration:**

```toml
[proxy_symbols.xauusd]
tradingview = "OANDA:XAUUSD"
twelvedata  = "XAU/USD"
source      = "tradingview"   # switch to "twelvedata" when TradingView session expires

[proxy_symbols.ihsg]
tradingview = "IDX:COMPOSITE"
twelvedata  = "COMPOSITE"
source      = "tradingview"

[proxy_symbols.dxy]
tradingview = "TVC:DXY"
twelvedata  = "DXY"
source      = "tradingview"
```

### Symbols configuration

```toml
[[symbols]]
symbol      = "BTCUSDT"    # exchange-native symbol
asset_class = "btc"        # btc | altcoin | gold | forex | stocks_idx | stocks_us
exchange    = "binance"    # binance | tradingview | twelvedata (affects default data_source)
data_source = "binance"    # explicit override (optional)
timeframes  = ["15m", "1h", "4h", "1d"]  # first entry = primary scan timeframe
```

Primary timeframe (first entry) drives candle fetch and confidence threshold selection.
Other timeframes are reserved for future multi-timeframe bias confirmation.

### WebSocket streaming mode

Default: `scanning_mode = "polling"` — fixed-interval REST fetches.

Event-driven streaming (sub-200ms signal latency):

```toml
[data_sources]
scanning_mode = "streaming"

[exchange.binance.ws]
enabled                    = true
url                        = "wss://stream.binance.com/stream"
max_streams_per_connection = 200
reconnect_base_delay_ms    = 1000
reconnect_max_delay_ms     = 30000
candle_buffer_size         = 500
```

Streaming fires on each closed kline event. In-progress bar updates are discarded.
Only symbols with `exchange = "binance"` receive streamed data; others receive no updates
in pure streaming mode.

---

## 7. Module Development Guide

### `src/indicators/`

- One module per indicator.
- All calculations must be **deterministic** for the same OHLCV input.
- RSI must use **RMA (Wilder smoothing)**, not SMA.
- ATR is the canonical distance unit — use `ATR × multiplier` for all thresholds and bands.
- Add `approx::assert_relative_eq!` tests against TradingView Pine Script reference values.
- Do not hardcode asset-specific behavior in generic indicator modules.

### `src/strategy/`

- Preserve `Long | Short | Wait | Freeze` semantics. Never skip Freeze.
- Trap guard runs before returning a Long or Short result.
- Confidence threshold + directional gap are both required.
- Asset-specific differences belong in `src/assets/` profile weights or config.
- Mark TP3 as optional in Telegram alerts.

### `src/exchange/`

- All future trading code calls the `Exchange` trait.
- Never call Binance/ByBit/MEXC/OKX SDK directly from trading, portfolio, risk, or backtest modules.
- `PaperExchange` (`paper.rs`) is the reference implementation for testing.
- `BinanceExchange` (`binance.rs`) is the Phase 2 scaffold — implements `Exchange` trait with
  `anyhow::bail!("not yet implemented")` stubs.

### `src/trading/`

Required production flow (Phase 2):
```
Signal → Risk::evaluate() → PositionSizer → Exchange::place_order() → OrderManager
```
Never bypass the risk layer. Kill switch must prevent orders until manually cleared.

### `src/backtest/`

The backtester must replay candles through the **same** indicator/strategy/risk/portfolio code
used by the scanner. Never fork indicator math for backtesting.
Backtest-specific code covers only: data loading, candle replay, simulated exchange/portfolio,
metrics, reports, optimization.

---

## 8. Testing

```bash
# Before any commit:
cargo fmt
cargo test
cargo build
cargo run -- check-config
```

Key test files:

| File | Coverage |
|---|---|
| `tests/indicator_parity.rs` | RSI/ATR/VWAP parity against TradingView fixture CSV |
| `tests/scanner_pipeline.rs` | End-to-end scan with mock data source |
| `tests/datasource_cli_api.rs` | Binance REST via mock server, Axum API routes, auth |
| `tests/alert_worker.rs` | Alert job claiming and deduplication |
| `tests/cache_valkey.rs` | Valkey key helpers, locks, pub/sub |
| `tests/storage_repositories.rs` | Signal/alert/order Postgres CRUD |

Baseline test for live data (requires network, marked `#[ignore]`):
```bash
cargo test baseline_candle_structure -- --ignored --nocapture
cargo test rest_sdk_parity          -- --ignored --nocapture
```

---

## 9. Development Workflow

### Before handing off any code change

```bash
cargo fmt
cargo test
cargo run -- check-config
cargo run -- scan-once          # verify no data-source errors
```

### Adding a new trading symbol

1. Add to `[[symbols]]` in `config/default.toml` with correct `asset_class`, `exchange`, `timeframes`.
2. Run `cargo run -- check-config` — verify symbol count increases.
3. Run `cargo run -- scan-once` — verify no `market_data_unavailable` in signal log.
4. Check first signal output: if `regime_block:Sideways`, market is consolidating — normal.

### Adding a new indicator

1. Create `src/indicators/<name>.rs`.
2. Add to `pub mod` in `src/indicators/mod.rs`.
3. Write unit test with `approx::assert_relative_eq!` against Pine Script reference.
4. Add `IndicatorPoint` or a named struct to the indicator snapshot in `src/strategy/signals.rs`.
5. Wire into asset profile weights in `src/config/profiles.rs` if confidence-scored.

---

## 10. Implementation Roadmap

| Phase | Scope | Status |
|---|---|---|
| 0–1 | CLI, Axum API, Postgres, Valkey, scanner pipeline, indicators, strategy, alerts | **Complete** |
| 1.5 | `binance-sdk` migration, MSRV 1.86, SDK-typed kline parsing, Phase 2 exchange scaffold | **Complete** |
| 2 | Live/paper trading, order lifecycle, fills, reconciliation, user-data stream | Pending |
| 3 | Portfolio/risk expansion, rebalancing, drawdown gates | Pending |
| 4 | Event-driven backtester, optimizer | Pending |
| 5 | 500+ symbol benchmark, latency tuning | Pending |
