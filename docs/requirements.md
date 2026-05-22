# Rust-Based Multi-Asset Trading Bot, Scanner, and Portfolio Manager: Complete Implementation Guide

Building a high-performance multi-asset trading bot, signal scanner, and portfolio manager in Rust that monitors technical indicators, delivers real-time trading signals via Telegram, exposes CLI and Axum web API controls, and can run 24/7 as a supervised service. This guide provides a complete architecture, implementation details, and best practices for creating a production-ready trading platform supporting Crypto (BTC, Altcoins, Gold tokens), Forex, and Stocks (US & Indonesian equities).

This document is aligned with `docs/architecture_audit.md`: Axum is the required web framework, Postgres is the durable database, and Valkey is the Redis-compatible cache/pub-sub/lock layer.

Implementation status: Phase 0 and Phase 1 are complete in the current codebase. The service runtime, Axum health/config/scan API, Postgres and Valkey foundations, market-data ingestion, indicator suite, strategy scanner pipeline, persisted/cached signal outputs, and Telegram alert worker are implemented and covered by tests. Phase 2+ work remains for live exchange execution, full trading, portfolio/risk integration, and event-driven backtesting.

## Supported Asset Classes

| Asset Class | Examples | Data Source | Key Characteristics |
|---|---|---|---|
| **BTC / Large-cap Crypto** | BTCUSDT, ETHUSDT | Binance, TradingView, Yahoo Finance | Stable structure, HTF dominant, high liquidity |
| **Altcoins** | SOL, DOGE, PEPE, WIF, SUI, ARB, OP | Binance, TradingView, Yahoo Finance | Volatile, wick-heavy, fake breakouts, thin liquidity |
| **Gold Tokens** | PAXGUSDT, XAUTUSDT | Binance, TradingView, Yahoo Finance | Follows XAUUSD spot, macro/session driven |
| **Forex** | EURUSD, GBPJPY, USDJPY | TradingView, Yahoo Finance | Session-driven, spread-sensitive, HTF bias |
| **Stocks (US)** | AAPL, TSLA, SPY | TradingView, Yahoo Finance | Market hours, volume-driven, earnings sensitivity |
| **Stocks (IDX)** | BBCA, TLKM, BMRI | TradingView, Yahoo Finance | IDX session gate, IHSG benchmark, broker flow |

## System Architecture and Design

The bot follows an **event-driven async architecture** powered by Tokio. The production system is a 24/7 service with a CLI, Axum web API, background workers, Postgres persistence, Valkey hot state/cache, and shared live/paper/backtest engines. The system comprises domain layers and infrastructure layers that work in concert to deliver real-time trading signals, portfolio/risk management, alerts, automated execution, and operational visibility.

## Production Stack

| Layer | Technology | Purpose |
|---|---|---|
| Async runtime | Tokio | Concurrent scanner, workers, API server, exchange calls |
| CLI | Clap | Operational commands (`serve`, `scan`, `trade`, `backtest`, `migrate`, `check-config`) |
| Web API | Axum | Fast HTTP API for health, status, controls, portfolio, signals, orders, risk |
| Durable database | Postgres | Signals, orders, fills, positions, balances, risk events, alerts, backtests, audits |
| Hot state/cache | Valkey | Scan snapshots, pub/sub, dedupe, distributed locks, rate limits, indicator cache |
| Logging | tracing | Structured service logs and task context |
| Exchange | Exchange trait | Platform-agnostic live, paper, and simulated execution |

Postgres is the durable source of truth. Valkey is a Redis-compatible hot-state layer and must not be the only storage for accounting, order, position, or kill-switch state.

## Core Components

**Data Layer**: The foundation fetches OHLCV (Open, High, Low, Close, Volume) data from multiple sources:

1. **`tvdata-rs`** — primary unofficial TradingView adapter for backend market-data ingestion. It is preferred over `tradingview-rs` because it has a broader backend-oriented surface for history, quotes, scanner/search/calendar workflows, auth modes, retry/request-budget configuration, and capability-aware validation. Use it behind the internal `MarketDataSource` trait, not directly from scanner/strategy code.
   - **Optional companion**: `tail-fin-tradingview` may be added later for feature-rich live WebSocket streaming, Pine/catalog tooling, or operational data exploration if `tvdata-rs` does not cover a required TradingView feature.
   - **Not preferred**: `tradingview-rs` is treated as legacy/backup research material and should not be the default project dependency for new TradingView datasource work.
2. **Binance HTTP/WebSocket adapter** — historical and near-real-time crypto market data for BTC, altcoins, and PAXG/XAUT gold tokens. Public klines do not require API credentials; account/trading endpoints require API key/secret.
   - **WebSocket streaming** (`BinanceWsStream`, `src/data/binance_ws.rs`): event-driven kline stream using Binance combined-stream endpoint. Enabled via `data_sources.scanning_mode = "streaming"` + `exchange.binance.ws.enabled = true`. Reduces signal latency from ~60 s (polling) to <200 ms (closed-bar event). Covers only symbols with `exchange = "binance"` in config.
3. **Yahoo Finance chart adapter** — fallback/supplementary historical OHLCV for daily/weekly/monthly data and proxy symbols (XAUUSD, DXY, IHSG-style benchmarks). No API key required.
4. **Forex data** — via TradingView or Yahoo Finance chart fallback for major/minor/exotic pairs.
5. **Proxy symbols** — XAUUSD spot for gold token validation, IDX:COMPOSITE (IHSG) for Indonesian equities benchmark, DXY for macro context. Fetched via TradingView or Yahoo Finance chart fallback.

This layer handles authentication, rate limiting, concurrent data fetching for multiple symbols, and session-aware scheduling (Asia/Europe/USA timezone gating). Each symbol's asset profile determines which data source is preferred, with fallback ordering: Binance (crypto) → TradingView via `tvdata-rs` → Yahoo Finance chart fallback.

**Resilience layer**: All HTTP adapters wrap their fetch calls in `data::retry::with_retry()` (exponential backoff, configurable via `[data_sources.retry]`). The Binance adapter additionally gates every request through an in-process `RateLimiter` to honour `exchange.rate_limit_per_second` and prevent HTTP 429 responses. WebSocket connections auto-reconnect with backoff on disconnect.

**Per-symbol data source routing** (`PerSymbolMarketData`, `src/data/mod.rs`): Each symbol is assigned a preferred data source via `SymbolConfig.data_source` or inferred from `exchange`. Proxy symbols (`ProxySymbolEntry`) carry separate TradingView and Yahoo Finance symbol identifiers with an explicit `source` preference and automatic Yahoo fallback. `PerSymbolMarketData.batch_candles` groups requests by source and executes Binance, TradingView, and Yahoo groups **concurrently** for maximum throughput.

**Service Runtime Layer**: Owns 24/7 operation. It starts and supervises the Axum API, scanner workers, alert workers, trading execution workers, reconciliation workers, and graceful shutdown. Live trading must remain disabled until startup reconciliation confirms exchange, Postgres, risk, and kill-switch state are consistent.

**API Layer**: Axum exposes operational and read-only endpoints for health, readiness, metrics, config inspection, signals, positions, orders, portfolio, risk state, manual scan triggers, trading enable/disable, and kill-switch controls. API handlers remain thin and delegate business logic to application services.

**Persistence Layer**: Postgres stores durable records: symbols, config snapshots, signal evaluations, alert jobs, alert deliveries, order state transitions, fills, positions, balances, portfolio snapshots, risk decisions, drawdown events, kill-switch state, backtest runs, reports, and audit events.

**Cache / Coordination Layer**: Valkey stores hot ephemeral state: latest scan snapshots, latest signal per symbol/timeframe, deduplication keys, worker heartbeat, pub/sub events, distributed locks, rate-limit buckets, cooldown markers, and indicator caches. Valkey accelerates the hot path but does not replace Postgres.

**Indicator Layer**: Technical indicators are implemented as deterministic internal modules for EMA, ATR/RMA, RSI, ADX/DMI, MACD, VWAP, volume, SMC structure, liquidity sweep, session-normalized volume, wick/body/CLV analysis, regime classification, and trap detection. Each indicator is modular and testable, with configurable parameters loaded from the TOML configuration file. Indicator weights are **asset-adaptive** — different asset classes emphasize different indicator combinations.

**Strategy Layer**: The signal detection engine uses a **multi-layer confidence scoring system** with asset-adaptive weighting. It evaluates trend structure (BOS/CHOCH), momentum (RSI/MACD/ADX), volume flow, trap risk, session context, and regime state. Signals are generated only when confidence exceeds asset-specific thresholds and no blocking conditions (trap, shock, chop) are active. The engine supports LONG, SHORT, WAIT, and FREEZE states.

**Alert Layer**: Telegram notifications are handled by the internal alert worker through the Telegram Bot API over `reqwest`. The worker reads queued alert jobs from Postgres, sends formatted messages, persists delivery attempts, and deduplicates repeated signal alerts through Valkey TTL keys. TP3 is always labelled optional.

Alert delivery should be queue-driven. The scanner emits alert jobs; an alert worker sends Telegram messages, persists delivery attempts, and deduplicates repeated signals.

**Scanner Loop**: The orchestration layer manages the async scanning process, coordinating concurrent symbol analysis while respecting API rate limits and handling errors gracefully.

The scanner hot path should fetch or read candles, compute indicators, score signals, update Valkey snapshots, enqueue side effects, and return quickly. It should not block every symbol task on non-critical Postgres writes.

**Asset Profile Engine**: Each symbol is tagged with an asset profile that determines indicator weights, confidence thresholds, session rules, and risk parameters. Profiles are loaded from TOML configuration.

**Exchange Layer**: A platform-agnostic trait interface (`Exchange`) abstracts order execution, balance queries, and market data. Binance is the default implementation; additional platforms (MEXC, ByBit, OKX) can be added by implementing the trait. A built-in paper exchange enables risk-free testing.

**Trading Engine**: Bridges strategy signals to exchange execution. Manages order lifecycle (place, partial fill, filled, cancelled), position scaling (EW1→EW2→EW3→Deep Add), TP/SL bracket orders, and trailing stops.

**Portfolio & Risk Layer**: Manages capital allocation across asset classes, enforces exposure limits, calculates position sizes, monitors drawdown, and provides a kill switch for emergency shutdown. The risk module can veto any trade that violates configured limits.

**Backtester**: An event-driven historical simulation engine that replays candles through the same indicator/strategy/risk pipeline used in live trading. Produces performance metrics (Sharpe, Sortino, max drawdown, win rate, profit factor) and supports parameter optimization.

---

## Technical Indicators Implementation

Detailed indicators and strategy used for this bot is described on `docs/indicators.md` and `docs/TV_Pine_Scripts/*.pine.txt` as TradingView's Pine Script v6 examples.

### 1. EMA (Exponential Moving Average) — Trend Direction

Three EMAs form the trend backbone:

| EMA | Period | Function |
|---|---|---|
| EMA Fast | 20 | Short-term momentum |
| EMA Slow | 50 | Medium-term trend |
| EMA Trend | 200 | Macro trend / structural filter |

**Signal interpretation**:
- Close > EMA20 > EMA50 → short-term bullish bias
- Close < EMA20 < EMA50 → short-term bearish bias
- Close > EMA200 → macro structure bullish
- Close < EMA200 → macro structure bearish
- EMA20/50 flat + RSI mid-range → sideways/chop risk

**EMA Formula**: EMA_today = (Price_today - EMA_yesterday) × (2/(period+1)) + EMA_yesterday

**Asset-specific weighting**:

| Asset | EMA Priority |
|---|---|
| BTC | H1/H4/D1 EMAs dominant |
| Altcoin | M1/M5/M15 EMAs for fast execution |
| Gold PAXG/XAUT | H1/H4/D1 EMAs (follows spot gold) |
| Forex | H4/D1 for bias, M15/H1 for entry |
| Stocks IDX | D1 EMA200 as primary trend filter |

### 2. ATR (Average True Range) — Volatility & Risk Engine

ATR is the backbone for all distance calculations in the system:

| Component | ATR Function |
|---|---|
| EW1/EW2/EW3 | Entry zone distances from anchor |
| Deep Add | Deep pullback entry distance |
| SL | Stop loss minimum/maximum distance |
| TP1/TP2/TP3 | Take profit targets |
| Trap detection | Wick size vs ATR ratio |
| Shock freeze | Candle range/body too large vs ATR → no trade |

**Default period**: 14

**Asset-specific behavior**:

| Asset | ATR Characteristic | Adaptation |
|---|---|---|
| BTC | Relatively stable | Standard ATR multipliers |
| Altcoin | Spikes suddenly | Compress EW/TP, widen SL buffer |
| Gold | Sensitive to news | Gold news shock filter |
| Forex | Session-dependent | Session-adjusted ATR |
| Stocks IDX | Intraday range-bound | Tighter ATR multipliers |

### 3. RSI (Relative Strength Index) — Momentum Health Filter

RSI is used as a **momentum health detector**, not a simple overbought/oversold signal.

**Formula**: RSI = 100 - (100 / (1 + RS)), where RS = RMA(Gain) / RMA(Loss) over 14 periods. Uses RMA (Relative Moving Average), not SMA, to match TradingView's implementation.

**Interpretation**:

| RSI Range | Meaning |
|---|---|
| 50–72 | Long momentum healthy |
| 28–50 | Short momentum healthy |
| 45–55 | Sideways / no clear momentum |
| RSI falling while price rising | Momentum decay (long weakening) |
| RSI rising while price falling | Momentum decay (short weakening) |

RSI is less stable on altcoins due to pump-dump cycles; more reliable on gold and forex due to cleaner macro moves.

### 4. ADX / DMI — Trend Strength

ADX measures trend strength; DMI (DI+/DI-) measures directional pressure.

| ADX Value | Meaning |
|---|---|
| < 13–15 | Chop / sideways → WAIT |
| 16–22 | Trend activating |
| > 22 | Trend strong enough for signals |
| Very high + extreme wick/volume | Exhaustion risk |

| DMI | Meaning |
|---|---|
| DI+ > DI- | Bullish pressure |
| DI- > DI+ | Bearish pressure |

**Default**: ADX Length 14, Smoothing 14.

For altcoins, high ADX can mean valid momentum OR pump trap — must combine with volume and wick analysis.

### 5. MACD — Momentum Confirmation

**Components** (12/26/9 default):
- MACD Line = EMA(12) - EMA(26)
- Signal Line = EMA(9) of MACD Line
- Histogram = MACD Line - Signal Line

**Signals**:

| Condition | Meaning |
|---|---|
| MACD > Signal + histogram positive | Bullish momentum |
| MACD < Signal + histogram negative | Bearish momentum |
| Histogram shrinking | Momentum weakening |
| MACD bullish but volume dropping | Fake move risk |
| MACD bearish but volume dropping | False breakdown risk |

MACD is a confirmation indicator, not primary. For altcoins it often lags — the LTF adapter and wick/trap engine take priority.

### 6. VWAP — Fair Value & Anti-Chase

VWAP (Volume-Weighted Average Price) serves as intraday fair value reference.

| Condition | Meaning |
|---|---|
| Close > VWAP | Buyers dominant |
| Close < VWAP | Sellers dominant |
| Reclaim VWAP | Potential bullish reversal |
| Reject at VWAP | Continuation bearish |
| Price far from VWAP | Mean reversion risk / no chase |

Also uses VWAP 1H as additional reference.

**Asset-specific importance**:

| Asset | VWAP Function |
|---|---|
| BTC | Intraday fair value |
| Altcoin | Detect chase and fake pump |
| Gold PAXG/XAUT | Validate token fair value vs spot |
| Forex | Less relevant (use session OHLC instead) |
| Stocks IDX | Intraday institutional fair value |

### 7. Volume Engine — Multi-Layer Flow Analysis

The volume system goes far beyond simple volume bars:

| Indicator | Function |
|---|---|
| Volume SMA | Baseline average volume |
| Volume Ratio | Current volume / average |
| Volume Z-Score | Detect extreme spikes |
| Session-Normalized Volume | Volume adjusted per Asia/Europe/USA session |
| Pressure Cluster | Detect repeated buy/sell pressure |
| Volume Decay | Detect weakening momentum |

**Session-normalized volume** is critical because:
- Normal USA volume looks large compared to Asia without normalization
- Small Asia spikes get misread as breakouts
- USA normal volume gets misread as shock

**Default**: Volume MA Length 20, Session Baseline Length 34, Shock Z-threshold 2.2, Breakout Ratio 1.15.

**Asset-specific**:

| Asset | Volume Engine Priority |
|---|---|
| BTC | Important for breakout confirmation |
| Altcoin | Mandatory — thin liquidity deceives |
| Gold PAXG/XAUT | Important, but must cross-reference XAUUSD |
| Forex | Tick volume only (use as relative filter) |
| Stocks IDX | RVOL (Relative Volume) primary; min avg value gate |

### 8. Candle Body, Wick Ratio, CLV — Price Action Engine

Detailed candle structure analysis:

| Component | Function |
|---|---|
| Candle body | Measures real move strength |
| Upper wick | Detects rejection / bull trap |
| Lower wick | Detects rejection / bear trap |
| Wick ratio | Wick dominance vs body |
| CLV (Close Location Value) | Whether close is near high or low |

**Interpretation**:

| Condition | Meaning |
|---|---|
| Large body + close near high | Valid bullish |
| Large body + close near low | Valid bearish |
| Large upper wick + close drops | Bull trap / stop hunt above |
| Large lower wick + close rises | Bear trap / stop hunt below |
| High volume + large wick | Manipulation risk |
| High volume + large body + strong CLV | Valid breakout |

For altcoins, the wick/body engine is often **more important than MACD/RSI**. For gold, critical during news events (CPI, NFP, FOMC, Powell speech).

### 9. SMC Structure — BOS, CHOCH, Swing Validation

Smart Money Concept structural analysis:

| SMC Component | Function |
|---|---|
| Pivot High/Low | Determine valid swing points |
| BOS Bullish | Close breaks above swing high |
| BOS Bearish | Close breaks below swing low |
| CHOCH Bullish | Character change from bearish to bullish |
| CHOCH Bearish | Character change from bullish to bearish |
| Swing Validation | Prevent noise/small swings from being treated as structure |

**Default**: Structure Lookback 18, Minimum Structure Score 60.

**Asset-specific**:

| Asset | SMC Notes |
|---|---|
| BTC | Very reliable |
| Altcoin | Must guard against fake BOS (uses `altFakeImpulse`, `altWickChaos`) |
| Gold | Valid when confirmed by XAUUSD proxy |
| Forex | Reliable on H1+ timeframes |
| Stocks IDX | Reliable with volume confirmation |

### 10. Liquidity Map — Equal High/Low, Sweep, Reclaim

Liquidity pool detection and sweep analysis:

| Indicator | Function |
|---|---|
| Equal High | Cluster of stop losses above (short stops) |
| Equal Low | Cluster of stop losses below (long stops) |
| Liquidity Sweep High | Price takes liquidity above then reverses |
| Liquidity Sweep Low | Price takes liquidity below then reverses |
| Reclaim after Sweep | Potential valid reversal |
| Two-bar Sweep Confirm | Don't trust single-candle sweeps |

**Practical interpretation**:

| Event | Meaning |
|---|---|
| High breaks equal high but close returns below | Bull trap / sweep above |
| Low breaks equal low but close returns above | Bear trap / sweep below |
| Sweep below + reclaim + healthy volume | Potential long |
| Sweep above + reject + healthy volume | Potential short |

**Default**: Liquidity Lookback 20, Equal Tolerance ATR 0.08.

### 11. Supply-Demand / Order Block

Order block detection from displacement candles:

| Component | Function |
|---|---|
| Bullish displacement | Strong up-candle after demand zone |
| Bearish displacement | Strong down-candle after supply zone |
| Bull OB | Potential demand area |
| Bear OB | Potential supply area |
| OB touched | Price enters OB zone |
| OB invalid | Close breaks through OB entirely |

**Asset-specific risks**:

| Asset | OB Reliability |
|---|---|
| BTC | Cleaner OBs |
| Altcoin | Often pierced by wicks then reverses |
| Gold | Valid when aligned with XAUUSD and active session |
| Forex | Reliable on H1+ |
| Stocks IDX | Reliable with volume confirmation |

### 12. Support / Resistance Cluster

S/R zones formed from swing clustering and ATR-based grouping:

| Function | Benefit |
|---|---|
| Nearest resistance | Prevents long entry just below a wall |
| Nearest support | Prevents short entry just above support |
| S/R block for TP | TP not placed beyond strong S/R without validation |
| Near S/R detection | Risk of rejection warning |

S/R cluster is critical for realistic TP placement. Many systems fail not because direction is wrong, but because **TP is placed beyond resistance/support that is too strong**.

### 13. Regime Classifier

Market condition detection that determines whether trading is allowed:

| Regime | Indicators | Bot State |
|---|---|---|
| Sideways | ADX low, EMA flat, RSI mid, small range | WAIT |
| Trend Expansion | ADX rising, ATR range rising, strong body | ACTIVE |
| Distribution Risk | Many upper wicks, momentum decay | Block long |
| Accumulation Risk | Many lower wicks, momentum decay | Block short |
| Shock / Liquidation | Range/body too large vs ATR | FREEZE (no trade) |

The regime classifier prevents forced trades during unfavorable conditions.

### 14. Session Engine — Asia, Europe, USA

Session-aware trading based on WIB (Asia/Jakarta) timezone:

**Functions**:
1. Adjust baseline volume per session
2. Adjust extra SL buffer per session
3. Adjust TP factor per session
4. Adjust EW reachability per session
5. Detect fake move risk per session

**Session characteristics**:

| Session | Character |
|---|---|
| Asia (06:00–14:00 WIB) | Thinner, range-bound, small fake moves |
| Europe/London (14:00–22:00 WIB) | Directional, liquidity increasing |
| USA/New York (19:00–03:00 WIB) | High volume, strong breakouts, but also large stop hunts and news shocks |

**Asset-specific session priority**:

| Asset | Most Important Sessions |
|---|---|
| BTC | USA and Europe dominant, Asia still active |
| Altcoin | USA largest moves, Asia prone to thin flow |
| Gold PAXG/XAUT | London and USA (follows gold spot) |
| Forex | London/NY overlap most liquid; avoid rollover (04:55–06:10 WIB) |
| Stocks IDX | IDX session only (09:00–15:00 WIB), pre-market gate |

**Default SL extras**: Asia +0.15 ATR, Europe +0.22 ATR, USA +0.30 ATR.

### 15. Multi-Timeframe (MTF) Engine

The scanner evaluates multiple timeframes simultaneously:

| Timeframe | Function |
|---|---|
| M1 | Micro trend / override (altcoin) |
| M5 | Entry confirmation |
| M15 | Intraday direction |
| 1H | Primary intraday structure |
| 4H | Swing trend |
| 1D | Macro direction |
| 1W | HTF dominance |
| 1M | Big macro bias |

**Asset-specific MTF weighting**:

| Asset | Primary Timeframes |
|---|---|
| BTC | 4H/1D very important |
| Altcoin | M1/M5/M15 more responsive; HTF as filter only |
| Gold | H1/H4/D1 + XAUUSD proxy more important than M1 |
| Forex | H4/D1 for bias, M15/H1 for execution |
| Stocks IDX | D1/W1 for trend, intraday for entry |

**Confidence thresholds per timeframe** (default):
- 15M: minimum 72% confidence
- 1H: minimum 67% confidence
- 4H: minimum 64% confidence
- 1D+: minimum 58% confidence

### 16. Trap Guard Engine

Multi-layer trap detection and protection:

| Engine | Function |
|---|---|
| Bull Trap Now | High sweep + close drops + upper wick |
| Bear Trap Now | Low sweep + close rises + lower wick |
| Equal High Sweep | Stop hunt above |
| Equal Low Sweep | Stop hunt below |
| Slow Stop Hunt | Gradual move toward liquidity pool |
| Wick Off | Repeated wicks against direction |
| Stealth Distribution | Many upper wicks + volume decay + RSI falling |
| Stealth Accumulation | Many lower wicks + volume decay + RSI rising |
| Shock Freeze | Candle too large → freeze all signals |
| Trap Cooldown | Wait N bars after trap before new signal |
| Two-bar Sweep Confirm | Don't trust single-candle sweep |

**Default**: Trap Score Threshold 60, Trap Volume Z 2.0, Wick Trap ATR 0.70.

When trap is detected, the system applies a **trap penalty** to the confidence score, potentially blocking the signal entirely.

### 17. Altcoin-Specific Adaptive Engine

Additional indicators for volatile altcoins:

| Indicator | Function |
|---|---|
| ATR% | Volatility relative to price |
| ATR Flow Ratio | Whether volatility is exploding |
| Range Flow Ratio | Whether candle range is expanding |
| Alt Wild Volatility | Altcoin in wild state |
| Alt Thin Flow | Volume/liquidity too thin |
| Alt Wick Chaos | Wicks too dominant |
| Alt Fake Impulse | High volume but weak body / large wick |
| Alt Clean Impulse | Valid breakout with body and volume |
| Alt Chaos | No trade when conditions too wild |
| Alt EW Factor | Auto-compress entry windows |
| Alt TP Factor | Auto-compress take profit targets |
| Alt SL Factor | Buffer SL for wick/volatility |
| Alt Trap Penalty | Extra probability penalty when trap-prone |

The altcoin engine answers: **"Is this move trustworthy or just a fake wick/pump?"**

### 18. Gold Token-Specific Engine

Additional indicators for PAXG/XAUT:

| Indicator | Function |
|---|---|
| XAUUSD Proxy | Compare token direction with gold spot |
| Gold Proxy Filter | Block trades against XAUUSD direction |
| Gold Proxy Weight | Extra score when aligned with XAUUSD |
| Gold Volatility Compression | Adjust EW/TP for gold volatility |
| Gold Thin Token Flow | Detect illiquid token pair |
| Gold News/Wick Chaos | No trade during wild news candles |
| London/USA Session Bias | Gold more valid during London/USA |
| Gold HTF Conflict Relax | Don't easily trade against H1/H4/D1 |

**Why XAUUSD proxy is mandatory**: PAXG and XAUT are gold tokens — their price should follow gold spot. If the token signals long but XAUUSD is strongly bearish, the token signal is suspect (spread, liquidity mismatch, delayed pricing, or temporary premium/discount).

### 19. Stocks IDX-Specific Engine

Additional indicators for Indonesian equities:

| Indicator | Function |
|---|---|
| IDX:COMPOSITE Benchmark | Compare stock vs IHSG direction |
| Min Avg Value Gate | Filter out illiquid stocks |
| IDX Session Gate | Only trade during IDX market hours |
| RVOL (Relative Volume) | Minimum 1.2x for valid buy |
| CMF (Chaikin Money Flow) | Smart money flow direction |
| OBV Slope | On-Balance Volume trend |
| Downside Risk Limit | Block entry when downside risk too high |
| Manual Broker Flow | Optional: retail sell dominant / big buy signals |

### 20. Forex-Specific Engine

Additional indicators for FX pairs:

| Indicator | Function |
|---|---|
| HTF Bias (H4/D1) | Primary directional filter |
| Session Filter | Only trade during active sessions |
| Rollover Avoidance | Block signals during spread-widening rollover |
| Spread Buffer | Account for broker spread in entry/SL |
| Range10/ATR Filter | Block choppy markets |
| ATR% Maximum | Block when intraday volatility too extreme |
| Tick Volume Filter | Relative tick volume as proxy |
| Counter-HTF Block | Block signals against daily/HTF bias |

---

## Signal Detection Strategy

The strategy engine uses a **weighted confidence scoring system** with asset-adaptive profiles. Rather than requiring all conditions to be met simultaneously, each indicator contributes to an overall confidence score. Signals are generated only when:

1. Confidence score exceeds the asset/timeframe threshold
2. Directional gap (long score - short score) exceeds minimum (default: 10)
3. No blocking conditions are active (trap, shock, chop, counter-trend)
4. Regime classifier allows trading (ACTIVE state)
5. Session is valid for the asset class

### Signal States

| State | Meaning |
|---|---|
| **LONG** | Buy signal active with entry plan |
| **SHORT** | Sell signal active with entry plan |
| **WAIT** | No clear signal; conditions not met |
| **FREEZE** | Shock/liquidation event; no trading allowed |

### Asset-Adaptive Indicator Weights

Each asset class uses different indicator priorities for the confidence score:

**BTC / Large-cap Crypto** — Structure First:

| Rank | Indicator | Weight |
|---|---|---|
| 1 | 4H/1D Trend Structure (BOS/CHOCH) | 18% |
| 2 | EMA 20/50/200 MTF alignment | 15% |
| 3 | VWAP position | 12% |
| 4 | ADX/DMI trend strength | 12% |
| 5 | Volume flow (session-normalized) | 12% |
| 6 | RSI momentum health | 10% |
| 7 | MACD confirmation | 8% |
| 8 | Liquidity sweep context | 8% |
| 9 | S/R cluster proximity | 5% |

**Altcoins** — Anti-Trap & Volatility First:

| Rank | Indicator | Weight |
|---|---|---|
| 1 | M1/M5/M15 LTF consensus | 16% |
| 2 | Wick chaos / fake impulse detection | 15% |
| 3 | Volume ratio + thin flow filter | 14% |
| 4 | ATR expansion state | 12% |
| 5 | Liquidity sweep detection | 12% |
| 6 | VWAP (anti-chase) | 10% |
| 7 | BOS/CHOCH (anti-fake BOS) | 8% |
| 8 | EMA trend | 7% |
| 9 | RSI/MACD | 6% |

**Gold PAXG/XAUT** — Proxy & Session First:

| Rank | Indicator | Weight |
|---|---|---|
| 1 | XAUUSD proxy direction | 20% |
| 2 | H1/H4/D1 EMA structure | 15% |
| 3 | London/USA session flow | 14% |
| 4 | VWAP (token fair value) | 12% |
| 5 | ATR / news volatility | 10% |
| 6 | BOS/CHOCH | 10% |
| 7 | Liquidity sweep | 8% |
| 8 | ADX/MACD/RSI | 6% |
| 9 | Token pair volume | 5% |

**Forex** — Session & Risk-Reward First:

| Rank | Indicator | Weight |
|---|---|---|
| 1 | HTF Bias (H4/D1 trend) | 18% |
| 2 | Session context (London/NY active) | 15% |
| 3 | BOS/CHOCH structure | 14% |
| 4 | ADX trend strength | 12% |
| 5 | EMA alignment | 12% |
| 6 | Liquidity sweep | 10% |
| 7 | RSI momentum | 8% |
| 8 | MACD confirmation | 6% |
| 9 | Tick volume ratio | 5% |

**Stocks (IDX)** — Volume & Downside Guard First:

| Rank | Indicator | Weight |
|---|---|---|
| 1 | RVOL + value gate | 16% |
| 2 | IHSG benchmark alignment | 14% |
| 3 | EMA 20/50/200 structure | 14% |
| 4 | BOS/CHOCH structure | 12% |
| 5 | CMF / OBV smart money flow | 12% |
| 6 | ADX trend strength | 10% |
| 7 | RSI momentum | 8% |
| 8 | S/R cluster | 8% |
| 9 | Downside risk assessment | 6% |

## Buy Signal (LONG) Criteria

A LONG signal is generated when the following multi-layer evaluation passes:

**Layer 1 — Trend Confirmation** (must pass):
- HTF structure bullish (BOS bullish or CHOCH bullish detected)
- Price above key EMA (asset-dependent: EMA50 for BTC/Gold, EMA20 for Altcoin)
- Structure score >= minimum (default: 60)

**Layer 2 — Momentum Validation**:
- RSI in healthy long range (50–72) or crossing above 30 (reversal)
- MACD line above signal line OR histogram increasing
- ADX > 16 with DI+ > DI- (trend active and bullish)

**Layer 3 — Volume Confirmation**:
- Session-normalized volume ratio > breakout threshold (default: 1.15x)
- No volume decay pattern
- CLV positive (close near high)

**Layer 4 — Entry Trigger** (at least one):
- Price Channel breakout above upper channel with volume
- Liquidity sweep below (bear trap) + reclaim + healthy volume
- Pullback to EMA/VWAP support with bounce confirmation
- BOS bullish with clean impulse (body > wick, volume present)
- Order block touch at demand zone with displacement

**Layer 5 — Anti-Trap Filter** (must pass all):
- No bull trap detected in last N bars
- No stealth distribution pattern
- No shock freeze active
- Wick ratio acceptable (not wick-dominated candle)
- Not in trap cooldown period
- Alt: no fake impulse, no wick chaos, no thin flow (altcoin only)
- Gold: XAUUSD proxy not bearish (gold only)
- Forex: not in rollover zone, not counter-HTF (forex only)
- IDX: RVOL >= 1.2x, not above max chase distance (stocks only)

**Layer 6 — Regime & Session Gate** (must pass):
- Regime classifier = ACTIVE or Trend Expansion (not Sideways/Shock)
- Current session valid for asset class
- No counter-trend block active (if enabled)
- Confidence score >= timeframe threshold (72/67/64/58 for 15M/1H/4H/1D)
- Directional gap (long - short) >= minimum gap (default: 10)

## Sell Signal (SHORT) Criteria

A SHORT signal is generated when the following multi-layer evaluation passes:

**Layer 1 — Trend Confirmation** (must pass):
- HTF structure bearish (BOS bearish or CHOCH bearish detected)
- Price below key EMA (asset-dependent: EMA50 for BTC/Gold, EMA20 for Altcoin)
- Structure score >= minimum (default: 60)

**Layer 2 — Momentum Validation**:
- RSI in healthy short range (28–50) or crossing below 70 (reversal)
- MACD line below signal line OR histogram decreasing
- ADX > 16 with DI- > DI+ (trend active and bearish)

**Layer 3 — Volume Confirmation**:
- Session-normalized volume ratio > breakout threshold (default: 1.15x)
- No volume decay pattern
- CLV negative (close near low)

**Layer 4 — Entry Trigger** (at least one):
- Price Channel breakdown below lower channel with volume
- Liquidity sweep above (bull trap) + rejection + healthy volume
- Rally to EMA/VWAP resistance with rejection confirmation
- BOS bearish with clean impulse (body > wick, volume present)
- Order block touch at supply zone with displacement

**Layer 5 — Anti-Trap Filter** (must pass all):
- No bear trap detected in last N bars
- No stealth accumulation pattern
- No shock freeze active
- Wick ratio acceptable (not wick-dominated candle)
- Not in trap cooldown period
- Alt: no fake impulse, no wick chaos, no thin flow (altcoin only)
- Gold: XAUUSD proxy not bullish (gold only)
- Forex: not in rollover zone, not counter-HTF (forex only)
- IDX: downside risk not exceeding limit (stocks only)

**Layer 6 — Regime & Session Gate** (must pass):
- Regime classifier = ACTIVE or Trend Expansion (not Sideways/Shock)
- Current session valid for asset class
- No counter-trend block active (if enabled)
- Confidence score >= timeframe threshold (72/67/64/58 for 15M/1H/4H/1D)
- Directional gap (short - long) >= minimum gap (default: 10)

## Entry Window (EW) / Take Profit (TP) / Stop Loss (SL) Engine

### Entry Windows (EW1, EW2, EW3, Deep Add)

Entry zones are calculated from the anchor swing using ATR-based distances:

| Level | Default ATR Distance | Function |
|---|---|---|
| EW1 | 0.12–0.85 ATR | Primary entry zone |
| EW2 | 0.42 ATR | Secondary entry (deeper pullback) |
| EW3 | 0.78 ATR | Tertiary entry (deep pullback) |
| Deep Add | 1.12 ATR | Deep add position (aggressive) |
| Entry Zone Width | 0.15 ATR | Width of each entry zone |

**EW modifiers**:
- Session-reachable EW: adjusts based on session volatility
- Micro pullback EW: for strong session moves (0.08/0.20/0.36 ATR)
- Session open buffer: 0.05 ATR
- Altcoin: EW compressed automatically (Alt EW Factor)
- Gold: conservative, not too close, not too far

### Take Profit Targets (TP1, TP2, TP3)

TP is calculated from multiple factors:

| Level | Default ATR | Function |
|---|---|---|
| TP1 | 0.48 ATR | Realistic primary target |
| TP2 | 0.88 ATR | Extended target if flow supports |
| TP3 | 1.35 ATR | Optional target (not guaranteed) |
| Min TP Step | 0.22 ATR | Minimum distance between TP levels |
| Max TP1 Cap | 0.95 ATR | TP1 cannot exceed this |
| Max TP2 Cap | 1.45 ATR | TP2 cannot exceed this |
| Max TP3 Cap | 2.10 ATR | TP3 cannot exceed this |

**TP modifiers**:
- Capped by daily/weekly liquidity levels
- Capped by nearest S/R cluster
- Session TP factor (reduced in Asia, expanded in USA)
- Flow/liquidity TP cap
- Altcoin: TP compressed automatically (Alt TP Factor)
- Gold: conservative TP aligned with XAUUSD structure
- Forex: TP based on Risk-Reward ratio (0.8R / 1.4R / 2.2R)

**TP3 is only valid when**: flow is high, ADX supports, no nearby S/R, no trap, volume not decaying, session active.

### Stop Loss (SL)

SL is calculated from multiple protective layers:

| Component | Function |
|---|---|
| Swing High/Low | Structure-based SL anchor |
| VWAP reference | Dynamic SL reference |
| EMA reference | Trend-based SL |
| Base ATR distance | Minimum SL (default: 0.50 ATR) |
| Maximum SL distance | Reject setup if SL too far (default: 3.20 ATR) |
| Session extra ATR | Asia +0.15, Europe +0.22, USA +0.30 |
| Trap extra ATR | Additional buffer when trap risk elevated |
| Wick extra ATR | Buffer for wick-heavy conditions |
| Volatility extra ATR | Buffer for high-volatility regime |
| Deep add invalidation | SL for deep add positions |

**Asset-specific SL**:

| Asset | SL Character |
|---|---|
| BTC | Structure + normal ATR |
| Altcoin | Wider (wick-heavy), but reject setup if SL too far |
| Gold | Not as wide as altcoin, but needs news/wick buffer |
| Forex | Structure-based + spread buffer |
| Stocks IDX | Max risk % gate (default: 5%) |

---

## Implementation Details

### Project Structure

The project follows a modular architecture with clear separation of concerns:

```
src/
├── main.rs                 # Entry point, Tokio runtime, CLI subcommands
├── app/
│   ├── mod.rs              # 24/7 service kernel and dependency wiring
│   ├── supervisor.rs       # Worker supervision, restart/degrade policy
│   ├── shutdown.rs         # Graceful shutdown and cancellation tokens
│   ├── services.rs         # Application services used by CLI/API/workers
│   └── reconciliation.rs   # Startup/periodic exchange vs Postgres reconciliation
├── api/
│   ├── mod.rs              # Axum router construction
│   ├── routes.rs           # Route registration
│   ├── handlers.rs         # Thin HTTP handlers
│   ├── dto.rs              # Request/response DTOs
│   ├── auth.rs             # API auth middleware/extractors
│   └── errors.rs           # HTTP error mapping
├── config/
│   ├── mod.rs              # TOML config loader
│   ├── profiles.rs         # Asset profile definitions
│   └── defaults.rs         # Default configuration values
├── data/
│   ├── mod.rs              # Data layer orchestration
│   ├── tradingview.rs      # TradingView fetcher
│   ├── binance.rs          # Binance fetcher
│   ├── yahoo.rs            # Yahoo Finance chart fallback fetcher
│   └── proxy.rs            # Proxy symbol fetcher (XAUUSD, IHSG, DXY)
├── storage/
│   ├── mod.rs              # Postgres pool + repository registry
│   ├── migrations/         # SQL migrations
│   ├── signals.rs          # Durable signal records
│   ├── alerts.rs           # Alert jobs and delivery attempts
│   ├── orders.rs           # Orders and order state transitions
│   ├── fills.rs            # Exchange fills
│   ├── positions.rs        # Positions and position snapshots
│   ├── balances.rs         # Balances and portfolio snapshots
│   ├── risk_events.rs      # Risk decisions, drawdown, kill switch records
│   └── backtests.rs        # Backtest runs, metrics, reports
├── cache/
│   ├── mod.rs              # Valkey client wrapper
│   ├── keys.rs             # Stable key namespace and builders
│   ├── snapshots.rs        # Latest candle/indicator/signal snapshots
│   ├── locks.rs            # Distributed locks
│   ├── pubsub.rs           # Runtime pub/sub events
│   └── rate_limit.rs       # Valkey-backed rate-limit buckets
├── indicators/
│   ├── mod.rs              # Indicator trait + registry
│   ├── ema.rs              # EMA 20/50/200
│   ├── atr.rs              # ATR engine
│   ├── rsi.rs              # RSI (RMA-based)
│   ├── adx.rs              # ADX/DMI
│   ├── macd.rs             # MACD
│   ├── vwap.rs             # VWAP
│   ├── volume.rs           # Volume engine (session-normalized, z-score)
│   ├── candle.rs           # Body/wick/CLV analysis
│   ├── smc.rs              # BOS, CHOCH, swing structure
│   ├── liquidity.rs        # Equal high/low, sweep, reclaim
│   ├── order_block.rs      # Supply/demand zones
│   ├── support_resistance.rs # S/R cluster
│   └── regime.rs           # Regime classifier
├── strategy/
│   ├── mod.rs              # Strategy orchestration
│   ├── confidence.rs       # Weighted confidence scorer
│   ├── signals.rs          # Signal generation (LONG/SHORT/WAIT/FREEZE)
│   ├── trap_guard.rs       # Trap detection engine
│   ├── session.rs          # Session engine (Asia/Europe/USA)
│   ├── mtf.rs              # Multi-timeframe engine
│   ├── entry_plan.rs       # EW1/EW2/EW3/Deep Add calculator
│   ├── tp_engine.rs        # TP1/TP2/TP3 calculator
│   └── sl_engine.rs        # Stop loss calculator
├── assets/
│   ├── mod.rs              # Asset-specific adapters
│   ├── btc.rs              # BTC profile & overrides
│   ├── altcoin.rs          # Altcoin adaptive engine
│   ├── gold.rs             # Gold token engine (XAUUSD proxy)
│   ├── forex.rs            # Forex engine
│   └── stocks_idx.rs       # IDX stocks engine
├── exchange/
│   ├── mod.rs              # Exchange trait (platform-agnostic interface)
│   ├── binance.rs          # Binance implementation (default)
│   ├── mexc.rs             # MEXC implementation (future)
│   ├── bybit.rs            # ByBit implementation (future)
│   ├── okx.rs              # OKX implementation (future)
│   └── paper.rs            # Paper trading (simulated execution)
├── trading/
│   ├── mod.rs              # Trading engine orchestration
│   ├── executor.rs         # Order execution (market/limit/stop)
│   ├── position.rs         # Position tracking & management
│   ├── order_manager.rs    # Order lifecycle (open/partial/filled/cancelled)
│   └── scaling.rs          # EW1→EW2→EW3→Deep Add position scaling
├── portfolio/
│   ├── mod.rs              # Portfolio manager
│   ├── allocator.rs        # Capital allocation per asset/symbol
│   ├── balance.rs          # Balance tracking (available/locked/unrealized)
│   ├── exposure.rs         # Exposure limits & correlation tracking
│   └── rebalancer.rs       # Portfolio rebalancing logic
├── risk/
│   ├── mod.rs              # Risk management orchestration
│   ├── position_sizer.rs   # Position sizing (% risk, Kelly, fixed)
│   ├── drawdown.rs         # Drawdown monitoring & circuit breaker
│   ├── limits.rs           # Per-trade, per-symbol, per-asset, daily limits
│   ├── correlation.rs      # Cross-asset correlation risk
│   └── kill_switch.rs      # Emergency stop (max drawdown, API errors)
├── backtest/
│   ├── mod.rs              # Backtester orchestration
│   ├── engine.rs           # Event-driven backtest engine
│   ├── data_loader.rs      # Historical data loader (CSV, API, cached)
│   ├── sim_exchange.rs     # Simulated exchange (fills, slippage, fees)
│   ├── sim_portfolio.rs    # Simulated portfolio & balance
│   ├── metrics.rs          # Performance metrics (Sharpe, Sortino, etc.)
│   ├── report.rs           # Report generator (JSON, CSV, terminal)
│   └── optimizer.rs        # Parameter optimization (grid/random search)
├── alerts/
│   ├── mod.rs              # Alert dispatcher
│   ├── queue.rs            # Alert queue producer/consumer
│   └── telegram.rs         # Telegram formatter + sender
├── observability/
│   ├── mod.rs              # Health/readiness/metrics state
│   ├── health.rs           # Health and readiness checks
│   └── metrics.rs          # Prometheus-compatible metrics
└── scanner/
    ├── mod.rs              # Scanner loop orchestration
    └── rate_limiter.rs     # API rate limiting
```

### Configuration System

The bot uses TOML for configuration, providing a human-readable format that's easy to modify. The application ships with sensible defaults; users only need to override what they want to change.

Key configuration sections include:

- TradingView authentication credentials
- Binance API credentials (+ optional MEXC, ByBit, OKX credentials)
- **Asset profiles** (BTC, Altcoin, Gold, Forex, Stocks IDX) with per-profile indicator weights
- Symbol lists per asset class
- Scanner settings (interval, timeframe, history bars)
- Indicator parameters (periods, thresholds) — global defaults + per-profile overrides
- Strategy rules (confidence thresholds, directional gap, blocking rules)
- Session definitions (Asia/Europe/USA times in WIB)
- EW/TP/SL parameters per asset profile
- Alert settings (Telegram credentials, formatting, chart links)
- Proxy symbols (XAUUSD, IHSG, DXY)
- **Server settings** (Axum enable flag, host, port, request timeout, auth token env var)
- **Runtime settings** (scan interval, shutdown timeout, max symbol tasks, health staleness threshold)
- **Database settings** (Postgres URL env var, pool size, migration behavior)
- **Cache settings** (Valkey URL env var, key prefix, pool size, TTL)
- **Exchange settings** (platform selection, API mode, testnet toggle)
- **Trading settings** (auto-trade enable, order types, execution mode)
- **Portfolio settings** (total capital, allocation per asset, max positions)
- **Risk management** (max risk per trade, max daily drawdown, kill switch thresholds)
- **Backtest settings** (date range, initial capital, fee model, slippage, data source)

### Async Architecture, Service Runtime, and Concurrency

The service uses Tokio's async runtime to run the Axum API, scanner, trading workers, alert workers, reconciliation workers, and backtest jobs without blocking. The scanner uses bounded concurrency to scan multiple symbols while respecting data-source and exchange rate limits.

**Key async patterns employed**:

1. **Task Spawning**: Each symbol scan runs as an independent async task using `tokio::spawn`
2. **Task Joining**: Multiple symbol scans execute in parallel and are joined after bounded analysis tasks finish
3. **Bounded Channel Communication**: `mpsc` channels coordinate scanner publishing into Postgres persistence workers
4. **Error Handling**: Graceful degradation when individual symbol scans fail
5. **Rate Limiting**: Semaphores control concurrent API requests to respect TradingView/Binance limits
6. **Proxy Prefetch**: XAUUSD and IHSG data fetched once per cycle, shared across relevant symbols
7. **Supervised Workers**: Long-running scanner, alert, trading, reconciliation, and API tasks run under a supervisor
8. **Graceful Shutdown**: Shutdown signals drain in-flight work, stop new scans/orders, flush critical state, and close connections
9. **Startup Reconciliation**: Live trading is blocked until exchange orders, balances, positions, Postgres state, and kill-switch state are reconciled
10. **Backpressure**: Bounded queues prevent slow Telegram, database, or exchange operations from exhausting memory

The async design enables scanning 500+ symbols every minute while maintaining low CPU and memory usage.

### Axum Web API

The production service exposes a fast HTTP API using Axum. API handlers must remain thin: authenticate and validate requests, call application services, and return typed DTOs. Do not place trading, strategy, exchange, or SQL business logic directly inside route handlers.

Recommended endpoints:

| Endpoint | Purpose |
|---|---|
| `GET /health` | Process health, uptime, dependency status |
| `GET /ready` | Readiness gate for scanner/trading/API dependencies |
| `GET /metrics` | Prometheus-compatible metrics |
| `GET /config` | Sanitized config summary, no secrets |
| `GET /signals` | Latest and historical signal records |
| `GET /positions` | Current positions from Postgres/reconciled state |
| `GET /orders` | Orders and order state transitions |
| `GET /portfolio` | Portfolio allocation, exposure, balances |
| `GET /risk` | Risk state, drawdown, kill switch status |
| `POST /scan` | Trigger a manual scan cycle |
| `POST /trading/enable` | Enable paper/live trading after safety checks |
| `POST /trading/disable` | Disable trading immediately |
| `POST /risk/kill-switch` | Trigger or reset kill switch with audit trail |

All mutating endpoints require authentication and should write audit events to Postgres.

### Postgres Persistence

Postgres is required for durable service state. It stores the accounting and audit trail that must survive restarts:

- symbols and asset profile snapshots
- sanitized app config snapshots
- signal evaluations and fired signals
- alert jobs and delivery attempts
- order requests, order state transitions, cancellations, and errors
- fills and execution reports
- positions, balances, portfolio snapshots, and exposure snapshots
- risk decisions, drawdown events, daily limits, cooldowns, and kill switch state
- backtest runs, parameters, metrics, and generated reports
- operator/API audit events

Postgres writes that are not required for execution safety should run through bounded workers or queues. The per-symbol indicator/scoring hot path should not block on non-critical database writes.

### Valkey Cache, Pub/Sub, and Locks

Valkey is required as the fast Redis-compatible state layer:

- latest candle, indicator, and signal snapshots
- scan-cycle status and worker heartbeat
- alert deduplication keys
- cooldown markers
- pub/sub events between scanner, trading, alert, and API services
- distributed locks for one-active-scanner or one-active-live-trader guarantees
- rate-limit buckets for exchange and data provider calls
- short-lived indicator caches keyed by symbol/timeframe/candle identity

Valkey is not a replacement for Postgres. Durable trading, portfolio, risk, and audit data must be persisted in Postgres.

### Data Fetching

TradingView ingestion should use `tvdata-rs` as the primary unofficial TradingView dependency. Keep all TradingView calls behind `data::TradingViewDataSource` and the internal `MarketDataSource` trait so the scanner, strategy, trading, portfolio, and backtest modules never depend on a third-party TradingView crate directly.

Recommended `tvdata-rs` usage:

```rust
// Pseudocode only: keep the exact crate API isolated inside data/tradingview.rs.
let tv = TradingViewClient::from_config(&config.data_sources.tradingview).await?;
let candles = tv.history()
    .symbol("NASDAQ:AAPL")
    .interval("1D")
    .limit(250)
    .fetch()
    .await?;
```

`tail-fin-tradingview` is approved as an optional second TradingView dependency only when a feature requires richer live WebSocket streaming, Pine/catalog scraping, or TradingView tooling beyond the `tvdata-rs` adapter. Do not mix both crates in scanner hot-path code; put any secondary implementation behind the same internal trait.

Authentication can run in guest/public mode when supported. For authenticated or premium data, load browser-derived TradingView values through config/env vars only:

```toml
[data_sources.tradingview]
enabled = false
auth_token_env = "TRADINGVIEW_AUTH_TOKEN"
session_id_env = "TRADINGVIEW_SESSION_ID"
session_signature_env = "TRADINGVIEW_SESSIONID_SIGN"
device_token_env = "TRADINGVIEW_DEVICE_T"
```

Never commit TradingView cookies, browser session IDs, or auth tokens. Binance API key/secret are also loaded from config/env vars and are required only for authenticated exchange/account features; public Binance klines do not require credentials.

Yahoo Finance chart fallback requires no authentication:

```rust
// Pseudocode: implementation lives behind data::YahooDataSource.
let candles = yahoo.candles("GC=F", Timeframe::D1, 250).await?;
```

**Data source priority**: Binance (crypto real-time) → TradingView via `tvdata-rs` (all assets, intraday) → Yahoo Finance chart fallback (daily+ data, proxy symbols, no-auth scenarios).

### Error Handling and Resilience

The bot implements comprehensive error handling using Rust's `Result` type and the `anyhow` crate for error propagation.

**Error handling strategies**:

- Retry logic for transient API failures (exponential backoff)
- Fallback mechanisms for missing data (skip symbol, continue scan)
- Graceful degradation when indicators can't be calculated (insufficient history)
- Proxy symbol fallback (if XAUUSD unavailable, reduce gold proxy weight)
- Detailed logging via `tracing` crate
- Telegram error notifications for critical failures

### Alert System

Telegram provides an excellent notification platform with rich formatting capabilities. The current implementation uses the Telegram Bot API directly through `reqwest` behind the internal `AlertSink` trait, so alternate Telegram crates can still be introduced later without changing scanner code.

**Message Format**:

Alerts include:
- Signal type with emoji (🟢 LONG, 🔴 SHORT)
- Asset class tag (BTC / ALT / GOLD / FX / IDX)
- Symbol and current price
- Confidence score (percentage)
- Direction and regime state
- Entry plan: EW1, EW2, EW3, Deep Add levels
- TP1, TP2, TP3 targets with probability
- Stop Loss level
- Key indicator values (RSI, ADX, MACD, Volume ratio)
- Trap warnings (if any active)
- Session context
- Timestamp (WIB)
- Direct link to TradingView chart

The alert worker claims pending jobs from Postgres, applies Valkey TTL dedupe, sends the message, records a delivery attempt, and marks the job as delivered, failed, or deduplicated. Telegram failures are recorded and do not crash the scanner.

### Performance Optimization

Several techniques ensure the scanner and service runtime operate efficiently:

1. **Concurrent Processing**: Async tasks run in parallel, dramatically reducing total scan time
2. **Indicator Caching**: Calculations cached in memory/Valkey to avoid redundant computation across timeframes
3. **Batch API Calls**: Multiple symbols fetched in single request when possible
4. **Proxy Sharing**: XAUUSD/IHSG fetched once, shared across all relevant symbols
5. **Release Builds**: Compiled with optimizations (`--release`) for 10x+ performance improvement
6. **Memory Efficiency**: Rust's zero-cost abstractions provide C-like performance
7. **Incremental Updates**: On WebSocket mode, only recalculate affected indicators
8. **Hot/Cold Path Separation**: Indicator computation, signal scoring, and order routing stay memory-first; reporting, audits, and historical queries run through Postgres-backed cold paths
9. **Bounded Queues**: Scanner, alert, persistence, and trading workers use bounded channels to apply backpressure
10. **Non-Blocking Persistence**: Non-critical Postgres writes are queued to workers instead of blocking per-symbol scan tasks
11. **Valkey Coordination**: Distributed locks, deduplication, pub/sub, and rate-limit buckets use Valkey for low latency
12. **Startup Reuse**: Warm caches and persisted snapshots can reduce cold-start load, but startup reconciliation remains mandatory before live trading

Benchmark target: scan 500 symbols in ~2.5 seconds with full indicator suite.

### Testing Strategy

Comprehensive testing ensures reliability:

**Unit Tests**: Each indicator has dedicated tests verifying calculation accuracy against TradingView reference values

**Integration Tests**: End-to-end tests validate the complete scanning pipeline per asset class

**Backtesting**: Historical data testing validates strategy performance before live deployment

**Asset Profile Tests**: Verify that each asset profile produces expected signal characteristics

**Mock Testing**: TradingView/Binance API calls can be mocked for faster test execution

**API Tests**: Axum route tests cover authentication, validation, success responses, and error mapping

**Storage Tests**: Postgres repositories and migrations are tested against isolated test databases or test containers

**Cache Tests**: Valkey keys, TTLs, pub/sub messages, distributed locks, and deduplication behavior are tested

**Runtime Tests**: Service startup, graceful shutdown, worker supervision, reconciliation gates, and health/readiness states are tested

**Recovery Tests**: Restart from existing Postgres state, reconcile open orders/positions, and verify kill switch persistence

### Deployment Options

**Local Execution** (development/testing):
```bash
cargo run --release
```

**Service Mode**:
```bash
cargo run --release -- serve
```

`serve` starts the Axum API plus supervised scanner, alert, trading, reconciliation, and persistence workers according to configuration.

**Docker Deployment**: Multi-stage Dockerfile minimizes image size while including all necessary dependencies.

**Compose / Local Stack**: Development deployments should run the bot with Postgres and Valkey services.

**Systemd Service (Linux)**: For production servers, systemd provides automatic restart, logging, and resource management.

**Kubernetes**: Run the bot as a deployment with readiness/liveness probes mapped to Axum endpoints, and run Postgres/Valkey as managed services or separately operated stateful services.

**Cloud Platforms**:
- AWS ECS/Fargate: Serverless container execution
- Google Cloud Run: Auto-scaling containerized apps
- DigitalOcean: Simple app platform deployment

---

## Exchange Abstraction Layer

The trading system uses a **platform-agnostic exchange trait** so that the core trading, portfolio, and risk logic is decoupled from any specific exchange. Binance is the default implementation; additional exchanges can be added by implementing the trait.

### Exchange Trait Interface

```rust
#[async_trait]
pub trait Exchange: Send + Sync {
    // Account
    async fn get_balance(&self) -> Result<AccountBalance>;
    async fn get_positions(&self) -> Result<Vec<Position>>;

    // Orders
    async fn place_market_order(&self, req: &OrderRequest) -> Result<OrderResult>;
    async fn place_limit_order(&self, req: &OrderRequest) -> Result<OrderResult>;
    async fn place_stop_order(&self, req: &StopOrderRequest) -> Result<OrderResult>;
    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<()>;
    async fn get_open_orders(&self, symbol: &str) -> Result<Vec<Order>>;

    // Market data
    async fn get_ticker(&self, symbol: &str) -> Result<Ticker>;
    async fn get_orderbook(&self, symbol: &str, depth: u32) -> Result<Orderbook>;

    // Symbol info
    async fn get_symbol_info(&self, symbol: &str) -> Result<SymbolInfo>;
    async fn get_exchange_info(&self) -> Result<ExchangeInfo>;

    // Metadata
    fn name(&self) -> &str;
    fn supports_testnet(&self) -> bool;
}
```

### Supported Platforms

| Platform | Status | Module | Notes |
|---|---|---|---|
| **Binance** | Market data implemented, live trading pending | `data/binance.rs`, `exchange/binance.rs` | Public klines work; account/trading adapter is Phase 2 |
| **MEXC** | Future | `exchange/mexc.rs` | Spot + Futures |
| **ByBit** | Future | `exchange/bybit.rs` | Unified account, Spot + Derivatives |
| **OKX** | Future | `exchange/okx.rs` | Unified account, Spot + Futures |
| **Paper** | Built-in | `exchange/paper.rs` | Simulated execution for testing |

### Exchange Configuration

```toml
[exchange]
platform = "binance"          # "binance", "mexc", "bybit", "okx", "paper"
testnet = false               # Use testnet/sandbox API
api_key = ""
api_secret = ""
# Optional per-platform overrides
recv_window_ms = 5000
rate_limit_per_second = 10
```

The paper exchange simulates order fills with configurable slippage and fees, enabling safe testing without real capital.

---

## Automatic Trading Module

The trading module executes orders based on signals from the strategy engine. It bridges the gap between signal generation and actual position management on the exchange.

### Operating Modes

| Mode | Description | Config |
|---|---|---|
| **Signal Only** | Scanner + alerts, no execution | `trading.enabled = false` |
| **Paper Trading** | Simulated execution via paper exchange | `exchange.platform = "paper"` |
| **Live Trading** | Real execution on configured exchange | `trading.enabled = true` |

### Trading Engine Flow

```
Signal (LONG/SHORT) → Risk Check → Position Sizer → Order Builder → Exchange Execution → Position Tracker
```

1. **Signal received** from strategy engine with full entry plan (EW1/EW2/EW3, TP1/TP2/TP3, SL)
2. **Risk manager validates**: within daily limits, drawdown OK, exposure OK, kill switch not triggered
3. **Position sizer calculates**: lot size based on risk %, account balance, and SL distance
4. **Order builder creates**: entry order (market or limit at EW1), SL order, TP orders
5. **Exchange executor sends**: orders to the exchange via the trait interface
6. **Position tracker monitors**: fill status, partial fills, TP/SL hits, position lifecycle

### Position Scaling (EW System)

The trading module implements the entry window scaling strategy:

| Step | Trigger | Action |
|---|---|---|
| EW1 Entry | Signal fires, price at EW1 zone | Open initial position (e.g., 40% of planned size) |
| EW2 Add | Price pulls back to EW2 zone | Add to position (e.g., 30% of planned size) |
| EW3 Add | Price pulls back to EW3 zone | Add to position (e.g., 20% of planned size) |
| Deep Add | Price pulls back to Deep Add zone | Final add (e.g., 10% of planned size) |
| SL Hit | Price reaches stop loss | Close entire position |
| TP1 Hit | Price reaches TP1 | Close partial (e.g., 50%) |
| TP2 Hit | Price reaches TP2 | Close partial (e.g., 30%) |
| TP3 Hit | Price reaches TP3 | Close remaining |

### Order Types

- **Market**: Immediate execution at current price (for urgent entries/exits)
- **Limit**: Entry at specific EW price levels
- **Stop-Market**: Stop loss execution
- **OCO (One-Cancels-Other)**: TP + SL bracket orders (where exchange supports)
- **Trailing Stop**: Optional trailing SL after TP1 hit

### Trading Configuration

```toml
[trading]
enabled = false                    # Master switch for auto-trading
mode = "live"                      # "live" or "paper"
order_type = "limit"               # "market" or "limit" for entries
use_oco = true                     # Use OCO bracket orders if supported
trailing_stop_after_tp1 = true     # Enable trailing stop after TP1 hit
trailing_stop_atr = 0.50           # Trailing distance in ATR

[trading.scaling]
ew1_pct = 40                       # % of position at EW1
ew2_pct = 30                       # % of position at EW2
ew3_pct = 20                       # % of position at EW3
deep_add_pct = 10                  # % of position at Deep Add
tp1_close_pct = 50                 # % to close at TP1
tp2_close_pct = 30                 # % to close at TP2
tp3_close_pct = 20                 # % to close at TP3 (remaining)
```

---

## Portfolio Management Module

The portfolio module manages capital allocation, tracks positions across multiple symbols and asset classes, and enforces exposure limits.

### Portfolio Structure

```
Total Capital
├── Reserved (not tradeable, safety buffer)
├── Allocated to BTC positions
├── Allocated to Altcoin positions
├── Allocated to Gold positions
├── Allocated to Forex positions
├── Allocated to Stocks positions
└── Available (unallocated, ready for new positions)
```

### Capital Allocation

Each asset class receives a configurable percentage of total capital:

```toml
[portfolio]
total_capital_usdt = 10000.0       # Total trading capital
reserve_pct = 10                   # % kept as safety buffer (never traded)
max_open_positions = 10            # Maximum concurrent positions across all assets
max_positions_per_asset = 4        # Maximum positions per asset class

[portfolio.allocation]
btc_pct = 30                       # % of tradeable capital for BTC
altcoin_pct = 25                   # % for altcoins
gold_pct = 15                      # % for gold tokens
forex_pct = 15                     # % for forex
stocks_pct = 15                    # % for stocks
```

### Exposure Management

| Rule | Purpose | Default |
|---|---|---|
| Max single position % | Prevent over-concentration | 5% of total capital |
| Max per-symbol exposure | Limit per-symbol risk | 10% of total capital |
| Max per-asset-class exposure | Limit sector risk | allocation % cap |
| Max correlated exposure | Prevent correlated blowup | 40% of total capital |
| Max total exposure | Overall leverage limit | 80% of tradeable capital |

### Balance Tracking

The portfolio tracks three balance states:

- **Available**: Free capital ready for new positions
- **Locked**: Capital in open positions (margin + unrealized P&L)
- **Unrealized P&L**: Current profit/loss of open positions

### Rebalancing

Optional periodic rebalancing ensures allocation stays within target ranges:
- Triggered when any asset class deviates > threshold from target allocation
- Can be automatic or alert-only (notify user to rebalance manually)
- Respects open positions — only rebalances available capital

### Portfolio Configuration

```toml
[portfolio.exposure]
max_single_position_pct = 5.0      # Max % of capital in one position
max_per_symbol_pct = 10.0          # Max % exposure to one symbol
max_correlated_pct = 40.0          # Max % in correlated positions
max_total_exposure_pct = 80.0      # Max % of capital deployed

[portfolio.rebalance]
enabled = false                    # Auto-rebalance
threshold_pct = 10.0               # Trigger when deviation exceeds this
mode = "alert"                     # "auto" or "alert"
```

---

## Risk Management Module

The risk module is the safety layer that protects capital. It can veto any trade, reduce position sizes, or trigger emergency shutdown.

### Risk Hierarchy

```
Kill Switch (emergency stop)
└── Daily Drawdown Limit
    └── Per-Asset Exposure Limit
        └── Per-Trade Risk Limit
            └── Position Sizing
```

Higher-level rules override lower-level ones. If the kill switch triggers, all trading stops regardless of individual position status.

### Position Sizing Methods

| Method | Formula | Use Case |
|---|---|---|
| **Fixed % Risk** | `size = (capital × risk%) / SL_distance` | Default, most common |
| **Kelly Criterion** | `size = (win_rate × avg_win - loss_rate × avg_loss) / avg_win` | Aggressive, for proven strategies |
| **Fixed Lot** | `size = configured_lot_size` | Simple, for testing |
| **Volatility-Adjusted** | `size = (capital × risk%) / (ATR × multiplier)` | Adapts to market volatility |

Default: **Fixed % Risk** with 1-2% risk per trade.

### Drawdown Protection

| Level | Threshold | Action |
|---|---|---|
| Warning | Daily P&L < -3% | Alert user, reduce position sizes by 50% |
| Caution | Daily P&L < -5% | Block new positions, only allow exits |
| Critical | Daily P&L < -8% | Close all positions, trigger kill switch |
| Max Drawdown | Account < -15% from peak | Kill switch, require manual restart |

### Kill Switch

The kill switch is an emergency stop that halts all trading activity:

**Triggers**:
- Max daily drawdown exceeded
- Max account drawdown from peak exceeded
- Exchange API errors exceed threshold (connectivity issues)
- Manual trigger via Telegram command or config flag

**Actions when triggered**:
- Cancel all open orders
- Close all positions at market (configurable: close or hold)
- Disable auto-trading
- Send critical alert via Telegram
- Require manual restart (config flag reset)

### Per-Trade Risk Limits

| Limit | Default | Description |
|---|---|---|
| Max risk per trade | 2% | Maximum capital at risk in a single trade |
| Max SL distance | 3.2 ATR | Reject setup if SL too far (from strategy) |
| Min risk-reward | 1.5:1 | Reject if TP1/SL ratio below threshold |
| Max concurrent trades | 10 | Total open positions |
| Max trades per day | 20 | Prevent overtrading |
| Cooldown after loss | 5 min | Wait period after a losing trade |

### Correlation Risk

The risk module tracks correlation between open positions:
- Multiple altcoin longs during BTC dump = correlated risk
- PAXG long + XAUT long = same underlying exposure
- Reduce combined size when correlation is high

### Risk Configuration

```toml
[risk]
max_risk_per_trade_pct = 2.0       # % of capital risked per trade
position_sizing_method = "fixed_pct" # "fixed_pct", "kelly", "fixed_lot", "volatility"
min_risk_reward = 1.5              # Minimum TP1/SL ratio
max_trades_per_day = 20
cooldown_after_loss_secs = 300     # 5 minutes

[risk.drawdown]
warning_pct = 3.0                  # Daily drawdown warning
caution_pct = 5.0                  # Block new trades
critical_pct = 8.0                 # Close all + kill switch
max_account_drawdown_pct = 15.0    # From equity peak, kill switch

[risk.kill_switch]
enabled = true
close_positions_on_trigger = true  # Close all or just stop new trades
max_api_errors = 10                # Consecutive API errors before trigger
manual_restart_required = true     # Require config flag reset after kill
```

---

## Backtester Module

The backtester validates strategy performance using historical data before live deployment. It uses the **same indicator and strategy engine** as the live scanner/trading module — no separate implementation. This ensures backtest results accurately reflect live behavior.

### Design Principles

1. **Shared engine**: Backtester imports and uses the same `indicators/`, `strategy/`, `assets/` modules as live trading
2. **Event-driven**: Processes candles sequentially, simulating real-time bar-by-bar arrival
3. **Realistic simulation**: Models slippage, fees, partial fills, and order latency
4. **Configurable via TOML**: Same `config.toml` structure as live, with backtest-specific overrides
5. **Reproducible**: Given the same config + data, produces identical results

### Backtest Engine Flow

```
Historical Data → Bar-by-Bar Feed → Indicator Engine → Strategy Engine → Simulated Exchange
                                                                              ↓
Report Generator ← Metrics Calculator ← Simulated Portfolio ← Position Tracker
```

1. **Data loader** fetches historical OHLCV (from cached files, API, or CSV)
2. **Bar feeder** replays candles one at a time, simulating real-time arrival
3. **Indicator engine** calculates all indicators on available history (same code as live)
4. **Strategy engine** generates signals (same code as live)
5. **Simulated exchange** fills orders with configurable slippage and fees
6. **Simulated portfolio** tracks positions, balance, P&L (same logic as live portfolio)
7. **Risk manager** enforces limits (same code as live risk module)
8. **Metrics calculator** computes performance statistics
9. **Report generator** outputs results

### Simulated Exchange

The backtest exchange simulates realistic order execution:

| Feature | Description |
|---|---|
| Market orders | Fill at next bar's open + slippage |
| Limit orders | Fill when price touches limit level |
| Stop orders | Trigger when price crosses stop level, fill at stop + slippage |
| Partial fills | Optional simulation of partial fill scenarios |
| Fees | Configurable maker/taker fee rates |
| Slippage | Configurable fixed or ATR-based slippage model |
| Latency | Optional simulated order latency (N bars delay) |

### Performance Metrics

The backtester calculates comprehensive performance statistics:

| Metric | Description |
|---|---|
| Total Return % | Net profit / initial capital |
| CAGR | Compound Annual Growth Rate |
| Sharpe Ratio | Risk-adjusted return (annualized) |
| Sortino Ratio | Downside-risk-adjusted return |
| Max Drawdown % | Largest peak-to-trough decline |
| Max Drawdown Duration | Longest recovery period |
| Win Rate % | Winning trades / total trades |
| Profit Factor | Gross profit / gross loss |
| Average Win / Average Loss | Mean P&L of winners vs losers |
| Expectancy | Average profit per trade |
| Total Trades | Number of completed trades |
| Trades per Day | Average trading frequency |
| Best/Worst Trade | Largest single win/loss |
| Consecutive Wins/Losses | Longest streak |
| Recovery Factor | Net profit / max drawdown |
| Calmar Ratio | CAGR / max drawdown |

### Report Output

Reports can be generated in multiple formats:
- **Terminal**: Summary table printed to stdout
- **JSON**: Machine-readable full results
- **CSV**: Trade-by-trade log for spreadsheet analysis
- **HTML** (future): Visual charts and equity curve

### Parameter Optimization

The backtester supports parameter optimization to find optimal settings:

| Method | Description | Use Case |
|---|---|---|
| Grid Search | Test all combinations of parameter ranges | Small parameter space |
| Random Search | Sample random combinations | Large parameter space |
| Walk-Forward | Optimize on window, validate on next | Prevent overfitting |

Optimization targets: Sharpe ratio, profit factor, or custom objective function.

### Backtest Configuration

```toml
[backtest]
enabled = false                    # Run in backtest mode
start_date = "2024-01-01"          # Backtest start date
end_date = "2025-01-01"            # Backtest end date
initial_capital = 10000.0          # Starting capital (USDT)
data_source = "api"                # "api", "csv", "cached"
csv_path = "./data/historical/"    # Path for CSV data files
cache_data = true                  # Cache downloaded data locally

[backtest.fees]
maker_fee_pct = 0.02               # Maker fee (Binance default: 0.02%)
taker_fee_pct = 0.04               # Taker fee (Binance default: 0.04%)
slippage_model = "fixed"           # "fixed", "atr_based", "none"
slippage_bps = 5                   # Fixed slippage in basis points
slippage_atr_pct = 0.05            # ATR-based slippage (% of ATR)

[backtest.simulation]
fill_on_next_bar = true            # Market orders fill at next bar open
partial_fills = false              # Simulate partial fills
order_latency_bars = 0             # Simulated latency in bars

[backtest.optimization]
enabled = false
method = "grid"                    # "grid", "random", "walk_forward"
target_metric = "sharpe"           # Optimization objective
max_iterations = 1000              # For random search
walk_forward_window = 90           # Days per optimization window
walk_forward_step = 30             # Days per validation step

# Parameter ranges for optimization (example)
[backtest.optimization.params]
min_confidence_1h = { min = 60, max = 80, step = 5 }
tp1_atr = { min = 0.30, max = 0.80, step = 0.05 }
max_risk_per_trade_pct = { min = 1.0, max = 3.0, step = 0.5 }
```

### CLI Subcommands

The application supports multiple run modes via CLI subcommands:

```bash
# 24/7 service mode: Axum API + supervised workers
cargo run --release -- serve --config config.toml

# Validate configuration
cargo run --release -- check-config --config config.toml

# Run database migrations
cargo run --release -- migrate --config config.toml

# Scanner mode — signal alerts only
cargo run --release -- scan

# One-shot scanner cycle
cargo run --release -- scan-once --config config.toml

# Backtest mode — run historical simulation
cargo run --release -- backtest --config config.toml

# Backtest with optimization
cargo run --release -- backtest --optimize --config config.toml

# Live trading mode
cargo run --release -- trade --config config.toml

# Paper trading mode
cargo run --release -- trade --paper --config config.toml
```

---

## Best Practices and Recommendations

### Trading Discipline

1. **Paper Trade First**: Test thoroughly before risking real capital
2. **Risk Management**: Use stop-losses and position sizing; never exceed max risk per trade
3. **Multiple Confirmations**: The bot already requires multi-layer confirmation — trust the system
4. **Market Context**: Consider overall market conditions and news (the bot handles this via regime classifier)
5. **Regular Review**: Analyze signal quality and adjust parameters per asset class
6. **TP3 is Optional**: Never treat TP3 as guaranteed; only valid under ideal conditions

### Technical Recommendations

1. **Start Small**: Begin with 10-20 symbols per asset class, scale gradually
2. **Appropriate Timeframes**: Use 1H/4H for swing trading, M5/M15 for day trading
3. **Rate Limit Awareness**: Respect TradingView API limits (adjust scan interval)
4. **Asset-Specific Tuning**: Each asset class has different optimal parameters — use profiles
5. **Session Awareness**: Best signals come during active sessions for each asset
6. **Logging**: Enable detailed logging for debugging and signal quality analysis
7. **Version Control**: Track configuration changes and their impact on signal quality
8. **Postgres Discipline**: Use Postgres for durable state, migrations, and audit trails
9. **Valkey Discipline**: Use Valkey for hot state, locks, pub/sub, dedupe, rate limits, and short-lived caches
10. **Axum API Boundaries**: Keep handlers thin and put business logic in application services
11. **Reconciliation First**: Never enable live trading before startup reconciliation completes

### Security Considerations

1. **Credential Protection**: Never commit API keys to version control
2. **Environment Variables**: Use environment variables or secret stores for sensitive data
3. **Regular Updates**: Keep dependencies current for security patches
4. **Access Control**: Limit who can modify bot configuration or call mutating API endpoints
5. **Audit Trail**: Persist all operator actions, signals, orders, risk decisions, and kill-switch changes to Postgres
6. **API Authentication**: Require authentication for all mutating Axum endpoints
7. **Sanitized Config API**: Never expose API keys, tokens, secrets, or raw credentials through the API

---

## Development Timeline

Based on the full multi-asset implementation with 24/7 service runtime, API, persistence, cache, trading, portfolio, risk, and backtesting modules, estimated development time is **170-230 hours**:

**Phase 0 — Production Runtime Foundation (25-35 hours) — implemented**:
- Axum API skeleton, health/readiness, auth middleware: 6 hours
- Service kernel, worker supervision, graceful shutdown: 7 hours
- Postgres pool, migrations, base repositories: 8 hours
- Valkey client, key namespace, locks, pub/sub, TTL cache: 6 hours
- Observability and metrics foundation: 4 hours

**Phase 1 — Core Scanner (40-50 hours) — implemented**:
- Project setup and dependencies: 2 hours
- Data fetching module (TradingView + Binance + Yahoo Finance + proxy): 5 hours
- Core indicator implementations (EMA, ATR, RSI, ADX, MACD, VWAP, Volume): 10 hours
- Advanced indicators (SMC, liquidity, OB, S/R, regime, candle engine): 12 hours
- Strategy engine (confidence scorer, signals, MTF): 6 hours
- Trap guard engine: 4 hours
- EW/TP/SL engine: 4 hours
- Asset-specific engines (Altcoin, Gold, Forex, IDX): 8 hours
- Session engine: 3 hours
- Alert system (Telegram): 3 hours
- Configuration and asset profiles: 4 hours

**Phase 2 — Exchange & Trading (25-30 hours)**:
- Exchange trait abstraction: 3 hours
- Binance exchange implementation: 6 hours
- Paper exchange implementation: 3 hours
- Trading engine (executor, position tracker, order manager): 8 hours
- Position scaling (EW1→EW2→EW3→Deep Add): 3 hours
- Integration with strategy signals: 4 hours

**Phase 3 — Portfolio & Risk (20-25 hours)**:
- Portfolio manager (allocator, balance, exposure): 6 hours
- Risk management (position sizer, drawdown, limits): 6 hours
- Kill switch implementation: 3 hours
- Correlation tracking: 3 hours
- Integration with trading module: 4 hours

**Phase 4 — Backtester (25-35 hours)**:
- Backtest engine (event-driven bar replay): 6 hours
- Data loader (API, CSV, cache): 4 hours
- Simulated exchange (fills, slippage, fees): 5 hours
- Simulated portfolio & risk: 4 hours
- Metrics calculator: 4 hours
- Report generator (terminal, JSON, CSV): 3 hours
- Parameter optimizer (grid/random/walk-forward): 6 hours
- CLI subcommands: 2 hours

**Phase 5 — Testing & Polish (15-20 hours)**:
- Unit tests for all modules: 8 hours
- Integration tests (full pipeline): 4 hours
- API/storage/cache/runtime tests: 8 hours
- Documentation: 3 hours
- Deployment setup: 2 hours
- Buffer for unexpected issues: 10-16 hours

---

## Troubleshooting Common Issues

**"Insufficient data" Errors**: Increase `history_bars` in configuration to fetch more historical data. Some indicators (EMA200) need at least 200+ bars.

**TradingView Rate Limiting**: Reduce symbol count or increase `interval_seconds`. Use `tvdata-rs` request-budget/retry controls where available and batch API calls through the internal datasource adapter.

**Telegram Messages Not Sending**: Verify bot token and chat ID are correct. Check bot has permission to send messages to target chat.

**High CPU Usage**: Reduce concurrent symbol count or increase scan interval. Ensure using release build.

**RSI Calculation Discrepancies**: Use RMA (Relative Moving Average) instead of SMA for RSI calculation, matching TradingView's implementation.

**XAUUSD Proxy Unavailable**: If proxy symbol data fails, the gold engine reduces proxy weight and relies on token-native indicators. Log a warning.

**IDX Session Gate Blocking**: Ensure timezone is set correctly (Asia/Jakarta). IDX only trades 09:00-15:00 WIB.

**Altcoin False Signals**: If too many false signals on altcoins, increase trap sensitivity, reduce TP targets, or increase minimum confidence threshold.

**Forex Rollover Noise**: Ensure rollover avoidance zone (04:55-06:10 WIB) is correctly configured to block signals during spread widening.

---

## Conclusion

This Rust-based multi-asset trading bot combines high-performance async architecture with sophisticated technical analysis, automated execution, portfolio management, and comprehensive risk controls. The modular design with asset-adaptive profiles allows each market to be analyzed and traded with the most appropriate indicator weights and risk parameters.

**System capabilities**:
- **24/7 Service**: Supervised runtime with graceful shutdown, health/readiness, reconciliation, and worker supervision
- **Web API**: Axum API for operations, status, portfolio, risk, signals, orders, and controls
- **Scanner**: Real-time signal detection across 500+ symbols in ~2.5s
- **Trading**: Automated execution with position scaling (EW1→EW3), bracket orders, trailing stops
- **Portfolio**: Multi-asset capital allocation with exposure limits and rebalancing
- **Risk**: Per-trade sizing, drawdown protection, correlation monitoring, kill switch
- **Backtester**: Historical validation using the same engine as live, with parameter optimization
- **Persistence**: Postgres-backed durable records for signals, alerts, orders, fills, positions, balances, risk events, audits, and backtests
- **Hot State**: Valkey-backed cache, pub/sub, deduplication, locks, rate limits, and latest runtime snapshots

**Core design principles**:
- **BTC = structure first** (HTF BOS/CHOCH → EMA → VWAP → ADX)
- **Altcoin = anti-trap and volatility first** (LTF consensus → wick chaos → volume → ATR)
- **Gold PAXG/XAUT = XAUUSD proxy and session-macro first** (proxy → H1/H4/D1 → London/USA)
- **Forex = session and risk-reward first** (HTF bias → session → structure → ADX)
- **Stocks IDX = volume and downside guard first** (RVOL → IHSG → EMA → CMF/OBV)

**Platform architecture**: Exchange-agnostic via trait abstraction. Binance default, with modular support for MEXC, ByBit, OKX, and paper trading. Runtime operations are exposed through CLI and Axum, durable state is stored in Postgres, and hot ephemeral state is coordinated through Valkey.

The bot is a tool to augment human decision-making, not replace it. Successful trading requires combining these signals with risk management and market understanding. Always paper trade first, validate with the backtester, and scale gradually.
