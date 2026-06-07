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

Status: **Done (2026-06-02)**.

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

### 1.7.12 Configuration Additions — Done 2026-06-02

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

### 1.7.13 Parity Test Harness — Done 2026-06-03

Estimate: 6–8 hours.

Tasks:

- Add `tests/parity/btc_v619.rs` that loads a captured BTC 1D candle series + reference panel JSON (extracted from Pine) and asserts the Rust panel matches within tolerance.
- Add equivalent suites for Altcoin V62, Gold V1, Forex V58, IDX V5.
- Add a CLI helper `cargo run -- export-panel <symbol> <timeframe>` that prints the full panel as JSON for diffing against Pine outputs.
- Document the capture/replay process in `docs/manual.md` so future Pine updates can regenerate fixtures.

Acceptance:

- `cargo test parity` runs all five asset parity suites green.
- A documented diff command shows zero structural differences between Rust panel JSON and the captured Pine panel JSON.

### 1.7.14 Panel Pine-parity polish — Done 2026-06-04

Estimate: 6–8 hours total. Tracked from the OANDA:XAUUSD 1D diff session on
2026-06-03 (see `docs/manual.md` § panel diff workflow). Each gap below is
sized independently so the work can land in 2–3 commits.

Gaps to close (in recommended landing order):

- **Gap 1 — Entry plan on Wait paths (~2–3h). Done 2026-06-03.** Lifted the
  `EntryPlanCalculator` projection out of the `Long`/`Short` emission branch
  in `src/strategy/signals.rs::evaluate_inner`. A "MAP" plan is now computed
  once after confidence resolution and threaded into every downstream
  `Wait` short-circuit via `PanelInputs.plan`, so `entry_ideal`,
  `waktu_entry`, `sl_*`, `tp*_prob/label`, `eta_tp*`, `reclaim_price`, and
  the EW band prices populate on `NO TRADE` panels.
- **Gap 4 — NEXT TRADE / WAKTU ENTRY window width (~5 min). Done 2026-06-03.**
  `panel.rs::next_trade_window` (Wait branch) and `panel.rs::entry_window`
  both rewritten to a 1-bar window starting at the next bar open — matches
  the Pine reference panel exactly.
- **Gap 5 — FLOW text Pine-exact phrasing (~5 min). Done 2026-06-03.**
  Replaced the three `flow_text()` strings with Pine wording:
  `Sepi → fakeout risiko tinggi`, `Normal/sedang, TP harus realistis`,
  `Aliran kuat, TP boleh agresif`.
- **Gap 6 — `TRADE SCORE` `TRAP` status (~30 min). Done 2026-06-04.**
  Added a `TradeScoreStatus::TrapBlocked` variant in `src/strategy/panel.rs`
  that wins over `NoDirection` whenever `inputs.trap.blocks_signal` is set.
  The variant renders as `TRAP` in Pine-matching uppercase.
- **Gap 3 — Per-asset `SESI` annotations (~1–2h). Done 2026-06-04.**
  Threaded `AssetClass` and `PanelAssetExtras` into
  `panel.rs::session_text`. Gold panels now append `| THIN EXCHANGE` when
  the session is Asia / Europe / Rollover (no COMEX overlap) plus a
  `| XAU BELI` or `| XAU JUAL` chip driven by the XAUUSD proxy bias.
- **Gap 2 — Vote-ratio `BIAS` / `CONF` rendering (~3–4h). Done 2026-06-04.**
  Added `layers_passed` / `layers_total` to `DirectionEvaluation`, a
  `vote_ratio_pct` helper in `src/strategy/signals.rs`, and
  `long_vote_pct` / `short_vote_pct` on `PanelInputs`.
  `panel.rs::bias_text` / `conf_text` now render Pine vote-ratio
  percentages while the weighted analog `confidence` score still drives
  the `MIN ≥ threshold` gate. CONF on `Wait` paths mirrors the dominant
  side (no more `BELI X% | JUAL Y%` split) to match Pine.

Tasks:

- Implement Gap 1 + Gap 4 + Gap 5 in one commit; refresh the five
  `tests/fixtures/parity/*.panel.json` snapshots via
  `UPDATE_PARITY_FIXTURES=1 cargo test --test parity`.
- Implement Gap 6 + Gap 3 in a follow-up commit; refresh fixtures.
- Implement Gap 2 last; refresh fixtures.
- Document the closed gaps in `docs/manual.md` § parity diff workflow and
  re-run `cargo run -- export-panel OANDA:XAUUSD 1d` to verify the diff
  against the Pine reference shrinks toward zero.

Acceptance:

- `cargo run -- export-panel OANDA:XAUUSD 1d` populates `entry_ideal`,
  `waktu_entry`, `sl_*`, `tp*_prob/label`, `eta_tp*`, and `reclaim_price`
  on `Wait` paths.
- `next_trade_window` width is exactly one timeframe span on `Wait` paths.
- `flow_text` matches Pine strings byte-for-byte.
- `trade_score_status` reports `TRAP_BLOCKED` whenever
  `inputs.trap.blocks_signal` is set.
- `session_text` for Gold panels appends `THIN EXCHANGE` and the XAU
  buy/sell tag in the active session.
- `bias_text` / `conf_text` percentages match Pine's vote-ratio semantics
  on the captured OANDA:XAUUSD 1D fixture (within ±1 percentage point).
- `cargo test --test parity` passes against refreshed fixtures.

### 1.7.15 Panel price-field exposure — Done 2026-06-05

Estimate: ~1 hour. Driven by the second OANDA:XAUUSD 1D diff session
(2026-06-04): the JSON export populated `ew*_status`, `deep_status`,
`sl_atr_distance`, and `tp*_prob/label`, but never the actual prices, so
the Pine rows `EW1 = 4496.885 | TOUCHED`, `SL = 4764.866 | 2.64 ATR WIDE`,
and `TP1 = 4459.893 | 0%` had no price half to render on our side.

Scope:

- Added 8 new `Option<f64>` fields to `PanelReport` in
  `src/strategy/panel.rs`: `ew1_price`, `ew2_price`, `ew3_price`,
  `deep_price`, `sl_price`, `tp1_price`, `tp2_price`, `tp3_price`. Each
  is populated from the threaded `EntryPlan` (real or `map_plan` from
  sub-phase 1.7.14 Gap 1) — `ew*_price` are band midpoints,
  `deep_price` is the deep_add midpoint, `sl_price` mirrors
  `plan.stop_loss.price`, and the TP prices come from
  `plan.take_profits.tp1/tp2/tp3_optional`. Each field carries
  `#[serde(default)]` so older fixtures without these keys still parse.
- Rewrote `src/alerts/telegram.rs::format_panel` so the EW / DEEP RISK /
  SL / TP rows now read prices from the `PanelReport` directly via two
  new helpers `format_ew_row` / `format_deep_row`. Previously TP prices
  came from `signal.entry_plan` — that source is `None` on Wait paths so
  the prices vanished even though Gap 1 already supplied a `map_plan`.
- Hardened `tests/parity/common.rs::assert_panel_matches` to compare the
  serialized JSON text rather than the parsed `Value`.
  `serde_json::Value` re-parses floats with a slightly less precise
  algorithm than `f64::from_str`, so values like `65100.114285714284`
  round-trip through `Value` to a 1-ULP-different f64 and re-serialize
  as `65100.11428571429`, producing spurious "drift" against fixtures
  written from the same panel. Text comparison sidesteps the lossy
  Value roundtrip entirely.
- Refreshed all 5 `tests/fixtures/parity/*.panel.json` snapshots.

Acceptance:

- `cargo run -- export-panel OANDA:XAUUSD 1d` JSON now includes
  `ew1_price`, `ew2_price`, `ew3_price`, `deep_price`, `sl_price`,
  `tp1_price`, `tp2_price`, `tp3_price` whenever an entry plan or MAP
  plan is available.
- Telegram panel HTML now renders the full Pine row form, e.g.
  `<b>EW1</b> 4496.8850 · TOUCHED ✅`,
  `<b>SL</b> 4764.8660 · WIDE · 2.64 ATR`,
  `<b>TP1</b> 4459.8930 · 0% LOW PROB`.
- `cargo test --test parity` passes against refreshed fixtures.
- `cargo test --lib` still passes (156/156).

### 1.7.16 Pine V1 Gold deep-audit gap report — Drafted 2026-06-05; commits 1 (A+B+E) & 2 (C) Done 2026-06-06

Estimate: ~9 hours total across 4 commits. Status: **Drafted**. Discovered during the
third OANDA:XAUUSD 1D diff session (2026-06-05) — the user re-ran
`cargo run -- export-panel OANDA:XAUUSD 1D` against the refreshed Pine reference
and found that `bias_text`, `conf_text`, `trap_gate_text`, EW1/EW2/EW3 prices,
`reclaim_price`, `sl_price`, TP1/TP2/TP3 prices, and the `extras` block still
diverge from Pine. Pine source compared was
`docs/TV_Pine_Scripts/TV_GOLD_PAXG_XAUT_V1.pine.txt` (V1, 1525 lines).

**Reference Pine output** (captured 2026-06-04 screenshot, OANDA:XAUUSD 1D, SHORT bias):
- BIAS: `Bias : MAP SHORT 15%`
- CONF: `JUAL 15% | MIN 58%`
- SESI: `USA | 20:30-02:59 WIB | THIN EXCHANGE | XAU JUAL`
- TRAP GATE: `AKUMULASI`
- EW1: `4496.885 | TOUCHED`, EW2: `4522.262 | WAIT`, EW3: `4543.161 | WAIT`
- DEEP RISK: `4569.713 | MAP`
- ENTRY IDEAL: `4531.219`
- SL: `4764.866 | 2.64 ATR WIDE`
- TP1: `4459.893 | 0%`, TP2: `4437.561 | 0%`, TP3: `4415.230 | 0% LOW PROB`
- RECLAIM: `4513.082`

**Rust output 2026-06-05** (same symbol/timeframe):
- BIAS: `MAP SHORT 50%`, CONF: `JUAL 50% | MIN 58%`
- TRAP GATE: `LIQ SWEEP`
- EW1: `4448.74`, EW2: `4492.19`, EW3: `4529.43`, DEEP: `4564.60`
- ENTRY IDEAL: `4508.33` (formula matches; differs because EW1/2/3 inputs differ)
- SL: `4676.32 | 3.05 ATR WIDE`
- TP1: `4338.06`, TP2: `4315.31`, TP3: `4292.55`
- RECLAIM: `4564.60` (same value as `deep_price` — clearly wrong source)
- extras.xauusd_bias: `wait` (proxy snapshot not feeding panel)

#### Gap A — BIAS / CONF percentages (regression from 1.7.14 Gap 2; CRITICAL). Done 2026-06-06.

Pine `biasPercent`/`confText` use the **weighted analog score**
(`longConf = int(round(clamp(longScore, 0, 100)))`), NOT a layer vote ratio.

```
Pine:   biasPercent = strongestBiasIsLong ? longConf : shortConf
        confText = strongestBiasIsLong ? "BELI " + str.tostring(longConf) + "%"
                                       : "JUAL " + str.tostring(shortConf) + "%"
        longConf = int(round(clamp(longScore, 0, 100)))
        ; longScore is the weighted composition on lines 691-745
        ; (bias4h, bias1d, trendUp, MACD, RSI, DI, ADX, VWAP, MTF dominant,
        ;  structure, SMC bonus, antiChop penalty, micro-trend overrides,
        ;  consensus weighting, gold proxy weight)
```

Rust currently renders `layers_passed / layers_total * 100` (introduced in
sub-phase 1.7.14 Gap 2). Pine 15% means `shortScore ≈ 15` post-composition.
Rust 50% comes from "4 of 8 hard layers" — a different quantity entirely.

Files to change:
- `src/strategy/panel.rs::bias_text` / `conf_text` — revert to using
  `inputs.confidence.score(direction)` (the weighted analog) and drop
  the `long_vote_pct`/`short_vote_pct` arguments from the public surface.
- `src/strategy/panel.rs::PanelInputs` — remove the two `*_vote_pct` fields
  added in 1.7.14 (or downgrade to debug-only).
- `src/strategy/signals.rs` — remove `layers_passed`/`layers_total` tracking
  from `DirectionEvaluation` (or keep for telemetry but stop feeding the panel).
- `src/strategy/signals.rs::build_panel` — drop the two added parameters.

#### Gap B — RECLAIM source formula wrong (CRITICAL). Done 2026-06-06.

Pine line 1499:
```pine
reclaim = activeLong ? ew1 - ((ew1 - ew3) * 0.35)
                     : ew1 + ((ew3 - ew1) * 0.35)
```

Pine SHORT verify: `ew1=4497`, `ew3=4543` → `reclaim = 4497 + 0.35*(4543-4497) = 4513.1` ✓
matches screenshot.

Rust currently (`src/strategy/panel.rs:266`):
`let reclaim = midpoint(&plan.deep_add)`

That value duplicates DEEP RISK and is the wrong source entirely. The
`midpoint(deep_add)` now correctly lives in `deep_price` after sub-phase 1.7.15,
so the duplication is no longer needed. Replace with the Pine formula
based on `ew1`/`ew3` midpoints + the active direction.

Files to change:
- `src/strategy/panel.rs::build` — replace the `reclaim` computation.

#### Gap C — EW spacing missing altEWFactor and daily-timeframe clamps (HIGH). Done 2026-06-06.

Pine lines 400, 831-846:
```pine
altEWFactor = useGoldEngine ? f_clamp(
    (altWildVol ? altEWVolCompress : 1.0)
  * (altThinFlow ? altEWThinCompress : 1.0)
  * (goldSessionQuiet ? 0.92 : sess == "EROPA" ? 0.98 : sess == "USA" ? 0.95 : 1.0),
    0.58, 1.05) : 1.0

ew1MaxEff = ew1MaxATR * altEWFactor
ew2Eff    = (timeframe.isdaily ? math.min(ew2ATR, 0.34) : ew2ATR) * altEWFactor
ew3Eff    = (timeframe.isdaily ? math.min(ew3ATR, 0.62) : ew3ATR) * altEWFactor
deepEff   = (timeframe.isdaily ? math.min(deepATR, 0.88) : deepATR)
          * f_clamp(altEWFactor + 0.08, 0.50, 1.12)
```

Rust `src/strategy/entry_plan.rs:101-104`:
```rust
let ew1_max_eff = self.config.ew1_max_atr;
let ew2_eff     = self.config.ew2_atr;
let ew3_eff     = self.config.ew3_atr;
let deep_eff    = self.config.deep_add_atr;
```

No session compression, no daily clamp. On 1D USA Gold, Pine effective is
`ew2=0.323, ew3=0.589, deep=0.907`. Rust uses raw `0.42, 0.78, 1.12` — that's
~1.7× wider spacing, which is the dominant cause of all the EW/SL/TP price
divergence.

Files to change:
- `src/strategy/plan_context.rs::PlanContext` — add `pub alt_ew_factor: f64`,
  `pub is_daily: bool`.
- `src/strategy/signals.rs::build_plan_context` — populate both fields.
- `src/assets/mod.rs::AssetEvaluator` — new method
  `fn ew_compression_factor(&self, session: MarketSession, flow: FlowState,
   shock_active: bool) -> f64`. Default impl returns `1.0`. Gold overrides
  with the Pine `altEWFactor` formula. Altcoin V62 likely needs its own variant.
- `src/strategy/entry_plan.rs::calculate_from_context` — apply the factor and
  daily clamps to `ew2_eff`, `ew3_eff`, `deep_eff` before the EW2/3/Deep
  computations. Note Pine's `deepEff` uses an *inner* clamp on
  `altEWFactor + 0.08`, then multiplies — not the same as `clamp(altEWFactor, …)`.

Test impact: refresh all 5 `tests/fixtures/parity/*.panel.json` snapshots.

#### Gap D — Trap-type detector divergence (MEDIUM)

Pine line 784 priority order:
`BULL TRAP > BEAR TRAP > LIQ SWEEP > DISTRIBUSI > AKUMULASI > SLOW HUNT > STOP HUNT > BERSIH`.

Rust `panel.rs::trap_gate_text` matches that priority order exactly. So the
divergence isn't in the panel rendering — it's in the upstream conditions
(`liqSweepHighSMC`, `liqSweepLowSMC`, `eqHighSweep`, `eqLowSweep` in Pine vs
the `TrapType::LiqSweep` decision in `src/strategy/trap_guard.rs`). Pine fired
`AKUMULASI` (accumulationRisk-only); Rust fired `LIQ SWEEP` — the Rust sweep
detector is over-firing relative to Pine's SMC + equal-high/low gates.

Files to investigate:
- `src/indicators/liquidity.rs` — sweep detector vs Pine
  `liqSweepHighSMC`/`liqSweepLowSMC` (SMC trend-state required) and
  `eqHighSweep`/`eqLowSweep` (equal-high/low marker plus a sweep).
- `src/strategy/trap_guard.rs::detect_trap_type` — verify the gating logic
  feeding `TrapType::LiqSweep` requires both an equal-high/low context AND
  the SMC trend state to be aligned, not just any wick-out.

Likely lower-priority for now; the higher gaps dominate the visible panel
divergence and this one needs a separate side-by-side test fixture.

#### Gap E — BIAS prefix "Bias : " missing (TRIVIAL). Done 2026-06-06 (folded into Gap A's `bias_text` rewrite).

Pine line 1138: `biasFullText = "Bias : MAP " + biasDisplay + " " + str.tostring(biasPercent) + "%"`.

Rust currently emits `"MAP SHORT 50%"` — missing the `"Bias : "` prefix.

Files to change:
- `src/strategy/panel.rs::bias_text` — prepend `"Bias : "` to the Wait branch
  (and Long/Short branches if Pine uses it there too — check the Pine
  `signalDir == "BELI"` path before committing).

#### Gap F — extras.xauusd_bias = "wait" (MEDIUM)

Pine line 523: `goldProxyBias = f_bias(goldPx, goldProxyE20, goldProxyE50)` —
produces BELI / JUAL / SIDEWAYS from a real OANDA:XAUUSD candle stream feeding
20/50 EMAs. Pine SHORT screenshot shows `XAU JUAL` in the SESI row, meaning
the proxy bias was active.

Rust emits `xauusd_bias: "wait"` — meaning `SignalDirection::Wait`. Either
(1) the TradingView proxy fetch returned no OANDA:XAUUSD candles for this run
(session cookie issue or symbol mapping), (2) the indicator snapshot isn't
computing the proxy bias from the prefetched candles, or (3) the asset extras
aren't propagating from the snapshot to `PanelInputs.extras`.

Files to investigate (in order):
- `src/data/proxy.rs::fetch_once_per_cycle` — verify the OANDA:XAUUSD entry
  is in `proxy_symbols` and returns non-empty candles.
- `src/indicators/htf_bias.rs` + the `IndicatorSnapshot` build — verify the
  proxy bias is computed when proxy candles are available.
- `src/strategy/signals.rs` — verify `PanelInputs.extras.xauusd_bias` is set
  from the snapshot (not hard-coded to `Wait`).

#### Gap G — SL formula possibly missing session/wick padding (MEDIUM)

Pine lines 240-248:
```pine
shortStop = math.max(baseStop, math.max(structStop, deepStop))
   where structStop = swingHigh + atr*slStructATR + sessPad + trapPad + wickPad + volPad
         deepStop   = deepAdd + atr*(minSLDistanceATR + f_session_extra(sess))
```

Pine SL = 4765, deep = 4570 → SL is ~3.5 ATR past deep.
Rust SL = 4676, deep = 4565 → SL is ~2.0 ATR past deep.

Either the session/trap/wick/vol padding stack isn't fully replicated in
`StopLoss::from_context`, or the structStop swingHigh source differs.

Files to audit:
- `src/strategy/sl_engine.rs::StopLoss::from_context` — side-by-side
  against Pine `f_short_sl` / `f_long_sl`.

#### Gap H — TP formula missing altTPFactor (LOW)

Pine lines 881-886:
```pine
shortTP1Raw = shortTPBase - atr * tp1ATR * trendTPFactor * sessTPFactor * altTPFactor
```

Rust `tp_engine.rs:176-178, 219-221`:
```rust
let raw1 = base + atr * config.tp1_atr * trend_factor * session_factor;
```

Missing the `altTPFactor` (asset/Gold-specific compression). The
`trendTPFactor` and `sessTPFactor` already match Pine. The TP base
(`max(close, EW1)` for LONG, `min(close, EW1)` for SHORT) also matches.

Files to change:
- `src/strategy/plan_context.rs::PlanContext` — add `pub alt_tp_factor: f64`.
- `src/assets/mod.rs::AssetEvaluator` — new method
  `fn tp_compression_factor(&self, session, flow, shock) -> f64` with `1.0`
  default and Gold/Altcoin overrides per Pine.
- `src/strategy/tp_engine.rs::from_context` — multiply raw1/2/3 by
  `ctx.alt_tp_factor`.

#### Suggested commit sequencing

1. **B + E + A** (panel text only — no formula math) — restore weighted-analog
   BIAS/CONF, fix RECLAIM source, add `"Bias : "` prefix. Refresh 5 parity
   fixtures. Smallest blast radius. ~1h.
2. **C** (`altEWFactor` + daily clamps in `entry_plan.rs` + new
   `AssetEvaluator::ew_compression_factor`). Refresh fixtures. ~3h. Biggest
   formula change; expect noticeable shifts in all 5 fixture asset suites.
3. **G + H** (SL session/wick padding completeness + `altTPFactor`). Refresh
   fixtures. ~2h.
4. **F + D** (proxy bias plumbing for `xauusd_bias` + trap-type detector audit).
   Likely the longest tail because (F) involves the network/proxy layer and
   (D) requires fixtures for the LIQ SWEEP vs AKUMULASI conditions. ~3h.

Acceptance for the full sub-phase:

- `cargo run -- export-panel OANDA:XAUUSD 1D` produces BIAS/CONF percentages
  matching Pine within ±2 percentage points.
- RECLAIM and DEEP RISK are no longer equal; RECLAIM tracks the Pine formula
  on lines 1499.
- EW1/2/3 spacing on 1D USA Gold within ±10 USD of Pine reference values.
- SL distance in ATR units within ±0.3 of Pine SHORT (2.64 ATR Pine vs 3.05
  ATR current Rust).
- TP1/2/3 prices within ±5 USD of Pine reference.
- `extras.xauusd_bias` reflects the live OANDA:XAUUSD proxy direction.
- `trap_gate_text` matches Pine on the captured trap fixture set.
- `cargo test --test parity` and `cargo test --lib` both green after each
  commit.

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
