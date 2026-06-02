# Phase 1 — Pine Script Parity Plan (Scanner & Strategy)

Audit date: 2026-05-25.

This plan extends the completed Phase 1 with the work needed to make the scanner & strategy output match the TradingView Pine reference panels in `docs/TV_Pine_Scripts/*.pine.txt`. The audit driving this plan compared `src/scanner/mod.rs`, `src/strategy/*.rs`, `src/indicators/*.rs`, and `src/assets/*.rs` against the V61.9 BTC script, V62.0 Altcoin script, V1 Gold (PAXG/XAUT) script, V58 Forex script, and V5 IDX script.

The reference panel for BTC 1D produces these rows: PUTUSAN, TRADE SCORE, NEXT TRADE, BIAS, CONF, SESI, FLOW, EW1, EW2, EW3, DEEP RISK, TRAP GATE, ENTRY IDEAL, WAKTU ENTRY, SL (+ ATR width), TP1/TP2/TP3 (+ probability %), ETA TP1/TP2/TP3, RECLAIM. Current Rust output emits only `state + confidence + TP3 line`. Parity is approximately **15–20%**.

## Audit Summary

### Architecture Alignment

| Concern | Pine | Rust | Verdict |
|---|---|---|---|
| Six-layer pipeline | Additive weighted scores + directional gap | All booleans must pass + weighted score (`signals.rs:293-372`) | PARTIAL — too strict |
| MTF security fetch (M1/M5/M15/H1/H4/D/W/M) | `request.security` × 7 | Single primary timeframe (`scanner/mod.rs:104-110`) | MISSING |
| SMC trend state machine | Persistent BOS→CHOCH | One-shot per call (`smc.rs:18-75`) | MISSING |
| Trap cooldown counter | Adaptive 4/6/7 bars | No state | MISSING |
| Shock-freeze N-bar counter | `shockFreezeBars=5` | Single-bar `MarketRegime::Shock` (`regime.rs:55-67`) | PARTIAL |
| V61.4 kill switch (longKill/shortKill) | EW3-break + reclaim + MTF reverse | Absent | MISSING |
| V61.6 hard execution filter (TP prob ≥ 60, microTrend, stealth dist/acc, dynamic SL cap) | Multi-gate | Absent | MISSING |
| V61.7 total guard (consensus, two-bar sweep, target-block-SR, no-chase) | Active | Absent | MISSING |
| V61.8 flow-adaptive TP (LOW/MID/HIGH flow caps, prob penalty) | Active | Absent | MISSING |
| V62 Altcoin adaptive engine (AUTO/MAJOR/MID/MEME, altEW/TP/SL/Trap factors, altChaos no-trade) | Active | Absent (`assets/altcoin.rs` is 1-line philosophy stub) | MISSING |
| Proxy snapshot piping | XAUUSD/IHSG/DXY into evaluator | Fetched then discarded: `let _proxy_snapshot = ...` (`scanner/mod.rs:91-98`) | MISSING |
| Asset-specific evaluator branches | Gold proxy bias, Forex HTF bias, IDX RVOL/CMF/OBV/IHSG RS | `evaluate_direction` does not branch on `AssetClass` (`signals.rs:293-372`) — weights only | MISSING |
| Session boundaries (WIB) | Asia 07:00-14:59, Europe 15:00-20:29, USA 20:30-02:59 | Asia 06:00-14:00, Europe 14:00-22:00, USA 19:00-03:00 (`session.rs:20-30`) | DIVERGENT |

### Algorithm Fidelity

| Indicator | Status | Notes |
|---|---|---|
| RSI (Wilder/RMA) | MATCH | `rsi.rs:46-65` |
| ATR | MATCH | `atr.rs:42-69` |
| ADX/DMI | MATCH | `adx.rs:36-121` |
| EMA | MATCH | `ema.rs:28-56` |
| MACD | MATCH | `macd.rs:49-115` formula; layer logic ignores histogram & zero-cross (`signals.rs:445-457`) |
| Candle shape | MATCH | `candle.rs:24-60` |
| VWAP | PARTIAL | Bucket boundaries diverge from Pine session lines (`vwap.rs:64-75`) |
| Volume engine | PARTIAL | Global rolling SMA+z-score; Pine maintains separate Asia/Europe/USA EMAs (`*.pine.txt:291-331`) |
| Structure (BOS/CHOCH) | PARTIAL | No pivot validation, no `minSwingDistanceATR`, no persistent `smcTrendState` |
| Liquidity sweep | PARTIAL | No equal-high/low handling, no CLV reclaim gate |
| Order block | PARTIAL | No BOS confirmation, no volume validation, no mitigation tracking |
| Regime classifier | PARTIAL + bug | Missing Continuation/Compression/Mixed; **`allows_signals` only allows TrendExpansion** (`regime.rs:22-25`) — far stricter than Pine, Waits on most bars |
| Support/Resistance | PARTIAL | No daily/weekly H/L blend, no strength score, no "NEAR" status |
| EW/TP/SL | DIVERGENT | Symmetric ATR bands around `close`; no swing anchoring, no session-reachable, no liquidity caps, no flow factors, no probability scoring |
| Session classifier | DIVERGENT | WIB cutoffs off by 1–2 hours |
| CMF / OBV / RVOL value-gate / Relative-Strength | MISSING | No modules under `src/indicators/` |
| HTF bias engine (H4 + D1) | MISSING | Required for Forex |

### Report Output Coverage

| Pine panel row | Rust source | Status |
|---|---|---|
| PUTUSAN | `signal.state` (`signals.rs:33-38`) | PARTIAL — enum only, no panel text |
| TRADE SCORE (xx/100) | Closest is `confidence.long.max(short)`; no composite | MISSING |
| NEXT TRADE (window + EXP) | Not computed (no movePerBar/ETA) | MISSING |
| BIAS (MAP SHORT 32%) | Not produced | MISSING |
| CONF (JUAL 32% \| MIN 58%) | `signal.confidence` + `threshold_for_timeframe` (no text) | PARTIAL |
| SESI (USA \| 20:30-02:59 WIB) | Enum only (`session.rs:7-14`), no time-range string | PARTIAL |
| FLOW (Waspada stop hunt) | Not produced — no flow engine | MISSING |
| EW1/EW2/EW3 (price + TOUCHED/WAIT/MAP) | `entry_plan.ew{1,2,3}` PriceBand; no status enum | PARTIAL |
| DEEP RISK (price + MAP/DANGER/RECLAIM OK) | `entry_plan.deep_add`; no status | PARTIAL |
| TRAP GATE (SHORT BLOCK / LONG KILL / SHOCK FREEZE) | Not produced | MISSING |
| ENTRY IDEAL (weighted EW anchor) | Not produced | MISSING |
| WAKTU ENTRY (timestamp + EXP) | Not produced | MISSING |
| SL (price + "2.2 ATR WIDE") | `entry_plan.stop_loss{price,atr_distance}`; no width annotation, no dynamic max-SL cap | PARTIAL |
| TP1/TP2/TP3 (price + probability %) | `entry_plan.take_profits`; no probability scoring | PARTIAL |
| ETA TP1/TP2/TP3 | Not produced | MISSING |
| RECLAIM (price) | Not produced | MISSING |

### High-Impact Bugs & Divergences

1. **Regime gate too strict** (`signals.rs:82-87`) — short-circuits to `Wait` unless `regime == TrendExpansion`. With default thresholds most BTC-1D bars classify as `Sideways`; scanner emits `regime_block:Sideways` indefinitely. Pine only blocks counter-trend chop.
2. **`ew1_max_atr` is dead config** — `entry_plan.rs:47` uses `ew1_min_atr` for the center. Pine clamps `f_reach_long(rawAnchor, close, ATR, ew1MinATR, ew1MaxEff)`.
3. **EW/TP/SL anchor is raw `close`** (`signals.rs:143`). Pine anchors EW to `max(swingLow, dailyLow, close - ATR*ew1MaxEff)`; SL to `swingLow - structural pad`.
4. **Strict `>` between long/short conf** (`signals.rs:113-119`); a tie silently emits `Wait`. Pine uses `>=` with directional gap.
5. **RSI layer is essentially always-true** (`signals.rs:434-441`): `(50..=72) || rsi > 30` reduces to `rsi > 30`. Pine strict: `50 ≤ rsi ≤ 72`.
6. **MACD layer ignores histogram & zero-cross** (`signals.rs:445-457`). Pine requires `macdHist > 0 AND macdLine > macdSig`.
7. **Session WIB boundaries off** (`session.rs:20-30` vs `*.pine.txt:201,206`). Overlap 19:00-20:30 misclassified.
8. **Proxy snapshot fetched but dropped** (`scanner/mod.rs:91-98`). Gold/IDX cannot see XAUUSD/IHSG.
9. **Asset-specific weight keys are dead**: `xauusd_proxy`, `ihsg_benchmark`, `cmf_obv`, `downside_risk`, `htf_bias`, `rvol_value_gate` exist in `profiles.rs:48-88` but `add_layer` calls (`signals.rs:314-358`) never pass these keys.
10. **No CMF, no OBV, no RVOL value-gate, no relative-strength** — required for IDX.
11. **No HTF bias module** for Forex.
12. **`detect_order_blocks` body-ratio hard-coded** to `0.55` (`signals.rs:481`); Pine uses `obDisplacementATR=0.65` (distance-based).
13. **`detect_zones` tolerance widens both bounds** (`signals.rs:482`), creating ~0.4*ATR cluster windows. Pine `srClusterATR=0.35` controls aggregation, not zone width.
14. **`format_signal` emits no panel** (`alerts/telegram.rs:104-124`).

## Phase 1.7 — Pine Script Parity & Panel Report

Estimate: 80–110 hours.

Status: **Planned**.

Goal: bring `SignalResult` output to functional parity with the TradingView Pine panels for all five asset classes, fix architecture-level bugs uncovered by the audit, and deliver the full panel row set through the alerts and API surfaces.

### 1.7.1 Core Strategy Bug Fixes (Quick Wins)

Estimate: 6–8 hours.

Tasks:

- Relax `signals.rs:82-87` regime block to mirror Pine: only `MarketRegime::Shock` triggers Freeze; other non-trend regimes reduce score but do not block emission.
- Loosen tie-break in `signals.rs:113-119` to use `>=` with `min_directional_gap` instead of strict `>`.
- Tighten RSI layer (`signals.rs:434-441`) to strict `50 ≤ rsi ≤ 72` for long and `28 ≤ rsi ≤ 50` for short, drop fallback OR clause.
- Tighten MACD layer (`signals.rs:445-457`) to require both `macdHist > 0` (or `< 0` for short) AND `macdLine > macdSig` (or `<` for short).
- Wire `ew1_max_atr` into EW1 reachability clamp; deprecate single-min EW center.
- Replace hard-coded `0.55` in `signals.rs:481` `detect_order_blocks` call with config-driven `ob_displacement_atr` (default 0.65).
- Pass cluster aggregation tolerance (not zone half-width) to `detect_zones` (`signals.rs:482`).

Acceptance:

- Regression tests cover Wait-on-Sideways and Freeze-on-Shock semantics.
- Unit tests show RSI/MACD layer flips boolean only at Pine-strict thresholds.
- Scanner emits non-Wait signals on candle fixtures where Pine reference also fires.

### 1.7.2 Session & VWAP Boundary Alignment

Estimate: 3–4 hours.

Tasks:

- Update `session.rs:20-30` WIB cutoffs to Pine: Asia 07:00-14:59, Europe 15:00-20:29, USA 20:30-02:59. Preserve `RolloverAvoid` 04:55-06:10 and `Idx` 09:00-15:00.
- Mirror updated cutoffs in `vwap.rs:64-75` so session VWAP anchors at the same bar boundaries.
- Add fixture-driven session classifier tests covering 19:00, 20:30, 14:59, 15:00, 06:59, 07:00 WIB cases.
- Expose a `session_time_range_string()` helper for the SESI panel row.

Acceptance:

- Session enum and time-range string both match Pine `f_session_time_range` output.
- Session VWAP test confirms reset at the 20:30 WIB USA boundary.

### 1.7.3 Multi-Timeframe Data Pipeline

Estimate: 12–16 hours.

Status: **Done (2026-06-01)**.

Tasks:

- Extend `scanner::ingest` (`scanner/mod.rs:89-135`) to fan out CandleRequests for `[M1, M5, M15, H1, H4, D1, W1, M1mo]` per symbol (configurable per asset class which subset is required).
- Add `MtfCandles { primary: Vec<Candle>, m1, m5, m15, h1, h4, d1, w1, mn }` and thread into `ScanWorkItem`.
- Extend `IndicatorSnapshot` (`signals.rs:188-282`) to carry per-timeframe `bias / bos / adx / ema_trend` summaries.
- Implement `consensusLongScore` and `consensusShortScore` (V61.7) from M1/M5/M15 votes; expose `consensusGap` to the score gate.
- Implement `microBullOverride / microBearOverride` (V61.6) using 5-bar momentum on the lowest available timeframe.
- Add `htf_bias` and `ema_htf` actual gate logic; wire weights in `add_layer` calls (`signals.rs:314-358`).
- Bound MTF fetch concurrency per symbol; reuse `MarketDataSource::batch_candles`.

Acceptance:

- Integration test asserts scanner pulls all configured timeframes per symbol once per cycle.
- Snapshot includes non-null H4/D1 bias on a fixture chain that spans ≥ 200 D1 bars.
- Consensus score blocks emission when M1/M5/M15 disagree by more than `consensusScoreGap`.

### 1.7.4 Stateful Guard Counters

Estimate: 8–10 hours.

Status: **Done (2026-06-01)**.

Tasks:

- Move trap cooldown into a per-symbol persistent state object (`GuardState`) keyed in Valkey with TTL ≥ scan interval; load before evaluate, save after.
- Implement adaptive trap cooldown: `cooldown = trapScore ≥ trapScoreThr ? 4|6|7 (by score) : max(cooldown-1, 0)`.
- Add `shockFreezeBars` counter that decrements each bar after a Shock regime detection; signal stays Frozen until counter hits zero.
- Add `deepReclaimBars` counter for V61.4 kill switch — count bars since EW3 / deep-add break and reset on reclaim.
- Persist `smcTrendState` (BullishExpansion / BearishExpansion / ContinuationLong / ContinuationShort / Choch / Range) per symbol; transitions driven by `detect_structure` events with `minSwingDistanceATR` validation.
- Add `flipConfirmBars` counter for trend flips.

Acceptance:

- Guard state survives scan cycles (verified by mock cache).
- Shock freeze blocks emission for exactly `shockFreezeBars` bars on synthetic shock candles.
- SMC trend state transitions only on validated swings ≥ `min_swing_distance_atr`.

### 1.7.5 EW / SL / TP Engine Rewrite

Estimate: 14–18 hours.

Status: **Done (2026-05-26)**.

Tasks:

- Replace `EntryPlanCalculator::calculate(direction, anchor, atr)` (`entry_plan.rs:32-60`) with `calculate(direction, snapshot, guard_state)` where `snapshot` carries swing_low, swing_high, daily_low, daily_high, weekly_low, weekly_high, ATR, VWAP, EMA20/50/200, OB list, session, regime, ADX, flow_state.
- Port Pine helpers `f_reach_long`, `f_reach_short`, `f_cap_long`, `f_cap_short`, `f_long_tp_fix`, `f_short_tp_fix`, `f_long_tp_flow`, `f_short_tp_flow`.
- Compute EW1/EW2/EW3 from `rawAnchor = max(swingLow, dailyLow, close - ATR * ew1MaxEff)` (long) / mirror for short; clamp via `f_reach_*`.
- Implement session-reachable EW micro ladder (`ewMicro1/2/3ATR`) for fast sessions; honour `ewSessionOpenBufferATR`.
- Rewrite `StopLoss::from_atr` (`sl_engine.rs:13-31`) into `StopLoss::from_snapshot` that composes:
  - `sessPad = ATR * f_session_extra(sess)`
  - `trapPad = (trap || cooldown) ? ATR * slTrapExtraATR : 0`
  - `wickPad = wick ≥ ATR * wickATRTrap ? ATR * slWickExtraATR : 0`
  - `volPad = volShock ? ATR * slVolExtraATR : 0`
  - Structural stop = `min(swingLow - pad, min(VWAP, EMA20) - pad*0.5)`
  - Deep stop = `deepAdd - ATR * (minSLDistanceATR + sessExtra)`
  - Final `stop = min(baseStop, min(structStop, deepStop))` (long; mirror for short).
- Apply V61.6 dynamic SL reject: if `|stop - close| / ATR > maxSL{sess}ATR`, set `setupRejected = true`.
- Add wick-off cap using `wickOffLookback / wickOffBufferATR`.
- Add `f_long_tp_flow / f_short_tp_flow` so TP1/2/3 honour `lowLiqTP{1,2,3}MaxATR` / `midLiq…` based on flow classification.
- Apply `sessTPFactor`, `trendTPFactor`, `altTPFactor`, liquidity caps (daily/weekly H/L, local 20/50), `tpStepMinATR` clamps.
- Add per-TP probability via `f_prob_score(conf, distAtr, trapVal, sidewaysNow, shockNow, adxVal)`; also emit textual band (LOW PROB / OK PROB / HIGH PROB).
- Stamp `atr_distance` and a `wide_label` ("NORMAL" / "WIDE" when atr_distance > 2.0) on `StopLoss`.

Acceptance:

- Golden-fixture test compares Rust EW1/EW2/EW3/Deep/SL/TP1/TP2/TP3/probabilities against precomputed Pine values for a captured BTC 1D series within ±0.5%.
- `tp{1,2,3}_optional` carries probability and label fields end-to-end.
- Rejected setups surface as `Wait` with `reason = "sl_too_wide:<sess>"`.

### 1.7.6 Trap Guard & Flow Engine Expansion

Estimate: 8–10 hours.

Status: **Done (2026-05-28)**.

Tasks:

- Extend `trap_guard.rs:36-101` to include Pine's full trap component set: `volZ`, `rangeShock`, `wickATR`, `slowStopHunt`, `eqHighSweep`, `eqLowSweep`, `stealthDistribution`, `stealthAccumulation`. Each emits a sub-score; aggregate via Pine weights.
- Add `pressureClusterBars / pressureClusterMin / pressureVolRatio` V61.4 detector that returns block when `pressure_count ≥ min` against direction.
- Implement V61.8 Flow engine: classify `RENDAH / SEDANG / TINGGI` from `volRatio` and `atrRatio` over `flowLookback` bars; feed into TP cap selection and apply `lowFlowProbPenalty` to TP probability.
- Implement V61.6 stealth distribution/accumulation guard using `distributionRejectBars`.
- Implement V61.7 two-bar sweep confirm: require sweep + reclaim across two consecutive bars before accepting `sweep_entry`.
- Implement V61.7 target-block-SR: block TP when strong S/R within `targetClearATR` of the level.
- Implement V61.7 no-chase: if `|close - TP1| < ATR * noChaseTP1ATR` set entry `Wait` with reason `"no_chase_near_tp1"`.

Acceptance:

- Flow engine returns three discrete states with deterministic boundaries on a synthetic series.
- Two-bar sweep test rejects single-bar sweeps that Pine also rejects.
- Stealth distribution test triggers when 5 consecutive bars close in upper third of range with declining volume.

### 1.7.7 Indicator Gaps

Estimate: 12–16 hours.

Status: **Done (2026-05-28)**.

Tasks:

- Add `src/indicators/cmf.rs` (Chaikin Money Flow, length 20) for IDX `cmf_obv` gate.
- Add `src/indicators/obv.rs` (On-Balance Volume with slope detection) for IDX `cmf_obv` gate.
- Add `src/indicators/rvol.rs` (Relative Volume vs N-bar average) and `value_traded` gate (rupiah × volume) for IDX `rvol_value_gate`.
- Add `src/indicators/htf_bias.rs` (H4 + D1 bias aggregator) for Forex `htf_bias` gate.
- Add `src/indicators/relative_strength.rs` (price-vs-benchmark slope) for IDX RS-vs-IHSG.
- Replace `volume.rs:29-88` global rolling SMA with per-session EMA + deviation EMA matching Pine `asia/europe/usa VolEma + DevEma` (`*.pine.txt:291-331`). Keep global ratio as fallback when session unavailable.
- Add `validBullBreakout / validBearBreakout` detector: `close > eqHigh AND body ≥ 0.45 ATR AND volRatio ≥ sessVolBreakoutRatio` (and mirror for bear).
- Extend `support_resistance.rs` with daily/weekly H/L blend, `srResistanceStrength`, and `priceNearResistance` flag (`srNearATR`).

Acceptance:

- Fixture-backed tests for each new indicator within ±0.5% of TradingView reference.
- IDX scanner uses RVOL ≥ 1.20 gate, CMF positive for long, OBV slope ≥ 0 for long.
- Forex scanner blocks counter-HTF setups when `htf_bias` disagrees with primary timeframe.

### 1.7.8 Asset-Class Evaluator Branching

Estimate: 8–10 hours.

Status: **Done (2026-05-29)**.

Tasks:

- Replace `evaluate_direction(direction, asset_class, snapshot, config)` single function (`signals.rs:293-372`) with a trait dispatch: `AssetEvaluator` per asset class with default Six-Layer impl and per-class overrides.
- Move BTC structure-first ordering into `src/assets/btc.rs::BtcEvaluator`.
- Implement `src/assets/altcoin.rs::AltcoinEvaluator` with V62 adaptive engine: profile resolver (AUTO/MAJOR/MID/MEME from quote currency + market cap heuristic), `altEWFactor`, `altTPFactor`, `altSLFactor`, `altTrapPenalty`, `altChaos` no-trade gate, `altLTFLongEdge` / `altFastLongFlip`.
- Implement `src/assets/gold.rs::GoldEvaluator` with XAUUSD proxy bias dominance (requires proxy snapshot threading — see 1.7.9). Add `goldSessionBiasMode` (London/USA only).
- Implement `src/assets/forex.rs::ForexEvaluator` with HTF bias gate (H4 + D1), session × RR multiplier (1.15/1.08/1.05/0.88), `blockCounterHTF`, ADX gate.
- Implement `src/assets/stocks_idx.rs::StocksIdxEvaluator` with RVOL + CMF + OBV + IHSG RS + downside-risk + chase-guard gates.
- Wire the missing weight keys (`xauusd_proxy`, `ihsg_benchmark`, `cmf_obv`, `htf_bias`, `rvol_value_gate`, `downside_risk`, `atr_news`) into the corresponding asset evaluators so the `profiles.rs` table is no longer half-dead.

Acceptance:

- Same candle fixture produces measurably different signal scores per asset class (Gold vs Altcoin vs IDX).
- Gold setups Wait when XAUUSD proxy bias opposes the primary direction.
- IDX setups Wait when RVOL < 1.20 or value-traded < threshold.
- Forex setups Wait when HTF bias opposes primary.

### 1.7.9 Proxy Snapshot Plumbing

Estimate: 4–5 hours.

Status: **Done (2026-05-28)**.

Tasks:

- Change `Scanner::ingest` (`scanner/mod.rs:89-135`) to return `(Vec<ScanWorkItem>, ProxySnapshot)` instead of binding the snapshot to `_proxy_snapshot`.
- Add `proxy: ProxySnapshot` to `ScanWorkItem` and to `IndicatorSnapshot`.
- Add `xauusd_bias`, `ihsg_bias`, `dxy_bias` accessors derived from proxy candles.
- Expose proxy timeframe selection per asset (Gold uses D1 XAUUSD, IDX uses D1 IHSG, Forex uses D1 DXY).
- Add cache layer so proxy snapshot is fetched once per cycle and reused across all symbols that need it.

Acceptance:

- Integration test confirms one proxy fetch per cycle regardless of symbol count.
- Gold evaluator reads `snapshot.xauusd_bias` non-empty when proxy fetch succeeds.

### 1.7.10 Panel Report Data Structure

Estimate: 8–10 hours.

Status: **Done (2026-05-29)**.

Tasks:

- Define `PanelReport` struct attached to `SignalResult`:
  - `putusan_text: String` (NO TRADE / OK / LONG / SHORT / FREEZE)
  - `trade_score: u32` (0–100 composite from confidence – distAtr – trapVal + adxBonus – sideways – shock)
  - `trade_score_status: String` (NO DIRECTION / WEAK / OK / STRONG)
  - `next_trade_window: TimestampRange` + `expires_at: DateTime<Utc>`
  - `bias_text: String` (e.g. "MAP SHORT 32%", "BELI 64%", "SIDEWAYS")
  - `conf_text: String` (e.g. "JUAL 32% | MIN 58%")
  - `session_text: String` (e.g. "USA | 20:30-02:59 WIB")
  - `flow_text: String` (e.g. "Waspada stop hunt", "Sepi rawan fake move")
  - `flow_state: FlowState` (Low / Mid / High)
  - `ew1_status / ew2_status / ew3_status: EwStatus` (Touched / Valid / Watch / Map / DeepReclaim)
  - `deep_status: DeepStatus` (Map / Danger / ReclaimOk / Invalid)
  - `trap_gate_text: String` (e.g. "SHORT BLOCK", "LONG KILL", "SHOCK FREEZE 3 BAR")
  - `entry_ideal: f64` (weighted EW anchor: 0.10*EW1 + 0.35*EW2 + 0.55*EW3 for long; mirror)
  - `waktu_entry: TimestampRange` + `expires_at: DateTime<Utc>`
  - `sl_wide_label: SlWidth` (Normal / Wide); `sl_atr_distance: f64`
  - `tp1_prob / tp2_prob / tp3_prob: u32`; `tp1_label / tp2_label / tp3_label: String` ("LOW PROB" / "OK PROB" / "HIGH PROB")
  - `eta_tp1 / eta_tp2 / eta_tp3: TimestampRange`
  - `reclaim_price: f64`
- Compute `eta_*` using `movePerBar = ATR / bars_per_session` heuristic from Pine, with session-aware adjustments.
- Serialize `PanelReport` via serde for storage and API payloads.
- Add panel-row null handling (e.g., when no signal, all entry/exit fields are `None` but session/flow/conf stay populated).

Acceptance:

- Golden test compares serialized panel JSON against captured Pine panel values for ≥ 3 fixture symbols.
- `SignalResult::panel.as_ref().map(|p| p.trade_score)` returns the same integer Pine reports.

### 1.7.11 Alert & API Surfaces

Estimate: 6–8 hours.

Tasks:

- Rewrite `alerts/telegram.rs::format_signal` to render the full panel as a structured Telegram HTML message (table-like with each panel row on its own line, using emoji ✅/❌/⚠️ markers consistent with the asset's report style).
- Add per-asset panel variants where rows differ (IDX exposes RVOL/CMF/OBV/RS rows; Gold exposes XAU proxy row; Forex exposes HTF bias row).
- Extend `src/storage/signals.rs` SignalRecord to persist `panel_report: serde_json::Value`.
- Expose `GET /signals/:symbol` API returning the latest panel report JSON.
- Expose `GET /panel/:symbol` (Axum) returning a rendered HTML page for human inspection.

Acceptance:

- Telegram sender produces a panel screenshot-equivalent for BTC 1D matching the reference image rows.
- API endpoint returns 200 with full panel JSON.
- Persistence migration adds `panel_report jsonb` column without breaking existing rows.

### 1.7.12 Configuration Additions

Estimate: 3–4 hours.

Tasks:

- Add new TOML keys to `config/default.toml` and corresponding deserialization structs in `src/config/*`:
  - `[indicators]`: `session_volume_shock_z`, `session_breakout_volume_ratio`, `min_pullback_atr`, `bos_close_buffer_atr`, `choch_close_buffer_atr`, `min_swing_distance_atr`, `liquidity_equal_atr`, `ob_displacement_atr`, `ob_validation_vol_ratio`, `momentum_decay_bars`, `distribution_reject_bars`, `sr_cluster_atr`, `sr_near_atr`.
  - `[strategy]`: `consensus_score_gap`, `flip_confirm_bars`, `min_structure_edge`.
  - `[trap_guard]`: `shock_freeze_bars`, `shock_range_atr`, `shock_body_atr`, `pressure_cluster_bars`, `pressure_cluster_min`, `pressure_vol_ratio`, `deep_reclaim_bars`.
  - `[entry_plan]`: `ew_micro_1_atr`, `ew_micro_2_atr`, `ew_micro_3_atr`, `ew_session_open_buffer_atr`, `min_rr_trade`, `target_clear_atr`, `no_chase_tp1_atr`, `max_sl_asia_atr`, `max_sl_europe_atr`, `max_sl_usa_atr`, `wick_off_lookback`, `wick_off_buffer_atr`, `sl_trap_extra_atr`, `sl_wick_extra_atr`, `sl_vol_extra_atr`.
  - `[entry_plan.flow]`: `flow_lookback`, `low_flow_vol_ratio`, `high_flow_vol_ratio`, `low_flow_atr_ratio`, `high_flow_atr_ratio`, `low_liq_tp1_max_atr`, `low_liq_tp2_max_atr`, `low_liq_tp3_max_atr`, `mid_liq_tp1_max_atr`, `mid_liq_tp2_max_atr`, `mid_liq_tp3_max_atr`, `low_liq_min_step_atr`, `low_flow_prob_penalty`, `flow_trap_wick_ratio`.
  - `[entry_plan.probability]`: `min_tp1_prob`, `min_tp2_prob`.
  - `[assets.altcoin]`: `alt_ew_vol_compress`, `alt_tp_thin_compress`, `alt_sl_wick_buffer_atr`, `alt_trap_sensitivity`, `alt_min_break_body_atr`, `alt_max_chase_atr`, `alt_ltf_weight`, `alt_htf_relax`, `alt_profile` (AUTO / MAJOR / MID / MEME).
  - `[assets.gold]`: `gold_session_bias_mode`, `gold_news_window_atr`, `gold_proxy_min_alignment`.
  - `[assets.forex]`: `forex_rr_asia`, `forex_rr_europe`, `forex_rr_usa`, `forex_block_counter_htf`.
  - `[assets.stocks_idx]`: `idx_rvol_min`, `idx_cmf_length`, `idx_obv_slope_bars`, `idx_value_traded_min`, `idx_rs_min`, `idx_downside_risk_threshold`.
  - `[symbols]`: add a Forex symbol example so `AssetClass::Forex` is exercised by `check-config`.
- Update `docs/manual.md` with config reference.
- Update `AGENTS.md` "Default Parameters" section with new keys.

Acceptance:

- `cargo run -- check-config` succeeds with new keys.
- All new keys have defaults matching Pine inputs.
- Per-asset overrides resolve correctly.

### 1.7.13 Parity Test Harness

Estimate: 6–8 hours.

Tasks:

- Add `tests/parity/btc_v619.rs` that loads a captured BTC 1D candle series + reference panel JSON (extracted from Pine) and asserts the Rust panel matches within tolerance.
- Add equivalent suites for Altcoin V62, Gold V1, Forex V58, IDX V5.
- Add a CLI helper `cargo run -- export-panel <symbol> <timeframe>` that prints the full panel as JSON for diffing against Pine outputs.
- Document the capture/replay process in `docs/manual.md` so future Pine updates can regenerate fixtures.

Acceptance:

- `cargo test parity` runs all five asset parity suites green.
- A documented diff command shows zero structural differences between Rust panel JSON and the captured Pine panel JSON.

## Delivery Sequencing

Recommended order (dependencies first):

1. **1.7.1** Quick wins (unblocks signal emission immediately).
2. **1.7.2** Session boundaries (cheap, prevents downstream divergence).
3. **1.7.12** Config additions (enables all later sub-phases to read parameters from TOML instead of constants).
4. **1.7.10** Panel report struct (defines the output target everything else feeds into).
5. **1.7.4** Stateful guard counters (precondition for kill switches and shock freeze).
6. **1.7.7** Indicator gaps (CMF/OBV/RVOL/HTF bias/RS) — independent and parallelizable.
7. **1.7.9** Proxy snapshot plumbing (small, unblocks Gold/IDX).
8. **1.7.5** EW/SL/TP engine rewrite (biggest single piece).
9. **1.7.6** Trap guard & flow expansion.
10. **1.7.3** MTF data pipeline (large infra change; consider running parallel with 1.7.5).
11. **1.7.8** Asset evaluator branching (depends on 1.7.5–1.7.7 + 1.7.9).
12. **1.7.11** Alert & API surfaces (consumes 1.7.10).
13. **1.7.13** Parity test harness (gates the work as complete).

## Out of Scope (Tracked Elsewhere)

- Live Binance trading adapter expansion → Phase 2.
- Event-driven backtester parity → Phase 4.
- Portfolio capital allocation across the new probability/flow signals → Phase 3.
- Performance benchmark of the larger MTF fetch pattern → Cross-Phase Backlog.

## Gate

Phase 1.7 is complete when:

1. All five asset classes (BTC, Altcoin, Gold, Forex, IDX) emit a full `PanelReport` with every row from the reference Pine panel populated (or explicitly `None` with a documented reason).
2. The parity test harness (1.7.13) is green for all five assets against captured Pine fixtures.
3. `cargo test` and `cargo run -- check-config` both pass.
4. The Telegram alert and `GET /signals/:symbol` API surfaces both render the full panel.
5. `AGENTS.md` and `docs/manual.md` reflect the new module layout and config keys.
