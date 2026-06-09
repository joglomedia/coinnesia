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

### 1.7.16 Pine V1 Gold deep-audit gap report — Drafted 2026-06-05; commits 1 (A+B+E), 2 (C), and 3 (G+H) Done 2026-06-06

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

#### Gap G — SL formula possibly missing session/wick padding (MEDIUM). Done 2026-06-06.

Audit verdict: Pine's sess/trap/wick/vol pad stack **was** already wired
in `StopLoss::from_context` correctly. The real cause of the SHORT-SL
divergence (Pine 4.67 ATR vs Rust 2.0 ATR past close) was two
*non-Pine* constraints added previously plus a missing `altSLFactor`
multiplier:

- **Fixed**: applied Pine `altSLFactor` to the absolute max-SL cap
  (`close ± atr * maxSLDistanceATR * altSLFactor`, Pine lines 869-870).
  New `AssetEvaluator::sl_extension_factor(session, flow, shock_active)`
  trait method (default `1.0`), with Gold and Altcoin overrides per
  Pine V1 line 402 (clamp `[1.0, 1.55]`) and V62 line 390 (clamp
  `[1.0, 1.85]`). Plumbed through `PlanContext.alt_sl_factor` populated
  by `build_plan_context`.
- **Removed**: the `dyn_cap = ew1 ± atr * session_cap` clamp on `sl` —
  Pine uses `maxSLDynamic` (line 1023) only for the "SL WIDE" too-wide
  reject check, never to clamp the SL itself. The too-wide check stays.
- **Removed**: the `sl.min(ew3 ± atr * 0.05)` clamp — not in Pine,
  was tightening SL aggressively to within 0.05 ATR of EW3.

Both removals match Pine's `f_long_sl`/`f_short_sl` exactly: the SL
candidate is the max of base/struct/deep stops, then pushed out by the
wick override, then capped only by `maxSLDistanceATR * altSLFactor`.
Token-specific Pine signals (`altWickChaos`, `goldNewsShock`) remain
proxied as `shock_active` and `flow == Low` — refinement under Gap F.

#### Gap H — TP formula missing altTPFactor (LOW). Done 2026-06-06.

Added `AssetEvaluator::tp_compression_factor(session, flow, shock_active)`
trait method (default `1.0`) with Gold and Altcoin overrides per Pine
V1 line 401 (clamp `[0.55, 1.08]`) and V62 line 389 (clamp `[0.38, 1.10]`).
Plumbed through `PlanContext.alt_tp_factor`. Multiplied into the raw
TP1/2/3 distances in `TakeProfits::from_context` for both LONG and SHORT
branches — Pine `tpNRaw = base ± atr * tpNATR * trendTPFactor *
sessTPFactor * altTPFactor` (Pine lines 881-886).

BTC / Forex / IDX inherit the `1.0` defaults so their TP prices remain
unchanged. Gold under USA + Mid + no-shock evaluates to `1.0` per Pine
(no multiplier hits) — divergence only appears in chaotic / quiet
sessions. Altcoin USA + Mid + no-shock also evaluates to `1.0`.

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

### 1.7.17 Score composition Pine parity — Done 2026-06-09 (Drafted 2026-06-08; all 4 commits landed in one session)

Estimate: ~10-15 hours total across 4 commits. Status: **Drafted**. Surfaced
during the OANDA:XAUUSD 1D verification session on 2026-06-08: after
sub-phase 1.7.16 commits 1-3 (Gap A+B+E panel rendering, Gap C `altEWFactor`,
Gap G+H SL/TP factors), Rust still reports `Bias : MAP SHORT 37%` against
Pine's `Bias : MAP SHORT 100%`. The panel render is now correct (analog score,
Pine wording); the underlying `confidence.short` weighted analog itself
diverges because the Rust scorer collapses Pine's granular sub-layer
accumulation into 8 macro hard layers + 6 soft layers, and the per-asset
profile weight tables (`config/profiles.rs`) cap the maximum reachable score
at 100 even before Pine's penalty stack would saturate.

Pine source compared was
`docs/TV_Pine_Scripts/TV_GOLD_PAXG_XAUT_V1.pine.txt` lines 675-745 (V1 Gold
`longScore`/`shortScore` composition) plus the V61.6 micro-trend override
(lines 711-714), V61.7 consensus weighting (lines 717-721), and V63 Gold
direction adapter (lines 725-745).

#### Audit — what Pine actually does

Pine `shortScore` (line 691-745) accumulates 16+ weighted sub-layers. Each
sub-layer is an `if condition then +N else 0` or `if condition then +N else -M`
expression on a single named boolean, *not* a macro layer. The maximum
attainable raw `shortScore` is **roughly 150** (sum of all positive paths
including the micro-trend / consensus / Gold proxy contributions), which
saturates the `clamp(_, 0, 100)` ceiling. Pine designed the score to
saturate routinely on clean trend bars — that's why the Gold screenshot
shows `100/100`.

Full Pine accumulator (Gold V1, ported verbatim from script lines 691-745):

| # | Pine signal | Sub-layer weight | Sign |
|---|---|---|---|
| 1 | `bias4h == "JUAL"` (else SIDEWAYS → +5) | +18 / +5 | + |
| 2 | `bias1d == "JUAL"` (else SIDEWAYS → +4) | +14 / +4 | + |
| 3 | `trendDown` (close < EMA20 < EMA50) | +14 | + |
| 4 | `trend200ShortOK` (close < EMA200) | +10 | + |
| 5 | `macdShortOK` (MACD histogram negative + signal cross) | +10 | + |
| 6 | `rsiShortOK` (RSI in 30..55 range for short) | +10 | + |
| 7 | `diMinus > diPlus` | +8 | + |
| 8 | `adx ∈ [16, 55]` | +8 | + |
| 9 | `vwapShortOK` (close < vwap) | +10 | + |
| 10 | `mtfDominantBias == "JUAL"` (else SIDEWAYS → +4) | +12 / +4 | + |
| 11 | `structureShortOK` (BoS or CHoCH bearish) | **+8 / −14** | both |
| 12 | `smcShortBonus` (variable from SMC trend state) | +N | + |
| 13 | `microBearOverride` (V61.6) | +14 | + |
| 14 | `microBullOverride` (V61.6, opposite direction) | **−18** | − |
| 15 | `consensusShortScore × 0.22` (V61.7) | +variable | + |
| 16 | `consensusLongScore × 0.18` (V61.7, opposite) | **−variable** | − |
| 17 | `goldProxyBear × goldProxyWeight` (V63, Gold) | +variable | + |
| 18 | `goldProxyBull × goldProxyWeight` (V63, opposite) | **−variable** | − |
| 19 | `altShortReversalOK` (V63 LTF reversal allowance) | +8 | + |
| 20 | `(altChaos or altFakeImpulse) × altTrapPenalty` (V63) | **−variable** | − |
| 21 | `macroConflictShort and altShortReversalOK × 8 × altHTFRelax` | +variable | + |
| 22 | `useGoldProxyFilter and goldProxyBull and not altShortReversalOK` | **−8** | − |
| 23 | `antiChop` penalty | **−12** | − |
| 24 | `strongConflictShort` penalty | **−18** | − |
| 25 | `macroConflictShort` penalty | **−10** | − |
| 26 | `volShock` penalty | **−4** | − |

Raw range pre-clamp: `[-94, +150]`. Pine then `clamp(score, 0, 100)`.

#### Audit — what Rust currently does

`src/strategy/signals.rs::evaluate_direction` (lines 1057-1172) collapses
this into 8 macro hard layers + 6 soft layers and looks up a single weight
per macro key from `config/profiles.rs`:

| Rust macro layer | Pine sub-layers folded in | Gold profile weight |
|---|---|---|
| `trend` | structure (11) + EMA20/50/200 trend (3+4) | `structure=10` |
| `htf_bias` | bias4h (1) + bias1d (2) | profile lacks `htf_bias` key → **0** |
| `ema_htf` | close vs EMA-200 HTF | `ema_htf=15` |
| `momentum` | MACD (5) + RSI (6) + DI (7) + ADX band (8) | `momentum=6` |
| `volume` | (Pine has no single corresponding pass-through) | `token_volume=5` |
| `entry` | liquidity sweep + VWAP-side check (9) | `liquidity=8` + `vwap=12` |
| `anti_trap` | wick/range checks | (no Pine equivalent in score) |
| `regime_session` | session active for asset class (10 fold-in proxy) | `session=14` |
| `xauusd_proxy` (soft) | goldProxyBear (17) | `xauusd_proxy=20` |
| `cmf_obv` / `ihsg_benchmark` / `rvol_value_gate` / `downside_risk` / `atr_news` (soft) | other-asset signals | varied |

Gold profile weight table maxes at exactly 100:
`xauusd_proxy=20, ema_htf=15, session=14, vwap=12, atr_news=10,
structure=10, liquidity=8, momentum=6, token_volume=5 → sum = 100`.

The penalty stack from Pine items 14, 16, 18, 20, 22, 23, 24, 25, 26
(potential `−94`) is **entirely absent** from the Rust scorer. `evaluate_direction`
only subtracts `trap.penalty` (line 1161) at the end. Pine also has
**failure penalties** built into specific layers (item 11: `structureShortOK`
gives `+8` on pass but `−14` on fail) — Rust's `add_layer` simply skips on
fail, never subtracts.

The micro-trend override (items 13/14) and consensus weighting (items 15/16)
have **no Rust equivalent at all**. The Gold V63 direction adapter (items
17-22) is also absent.

#### Why this produces the observed `37%` vs `100%`

On the user's OANDA:XAUUSD 1D bar:
- Rust attainable max: 100 (profile ceiling).
- Rust observed: 37 → indicates roughly 4 of 9 weighted Gold layers fired.
- Pine attainable max: 150 (saturates clamp).
- Pine observed: 100 (saturated clamp) → indicates at least 16 of 26 Pine
  sub-layers fired, possibly with positive contributions overflowing the
  clamp by 20-50 raw points.

Even if Rust fired *every* macro layer it would only reach 100 — but the
profile-key macro mapping means many Pine sub-layers contribute nothing
(e.g. there's no `bias4h`/`bias1d` weight at all because `htf_bias` isn't
in the Gold profile table). And there's no penalty path, so Rust can't
distinguish a clean trend from a chop-on-conflict bar the way Pine does
through `antiChop`, `strongConflict`, `macroConflict`, `volShock`.

#### Implementation result (Done 2026-06-09)

Commit 1 — Sub-layer accumulator skeleton.
- `src/config/profiles.rs` rewritten with `WeightPair { pass, fail }`
  shape and `AssetProfile::weight()` lookup helper.
- All 5 asset profiles migrated to per-sub-layer keys: BTC has 17, Altcoin
  17, Gold 18, Forex 16, IDX 20.
- `src/strategy/signals.rs::evaluate_direction` replaced with an `add`
  closure that consults the profile and applies pass / fail weights
  per Pine V1 line 701 (`structureShortOK ? +8 : -14` is the canonical
  fail-weight example).
- 17 new sub-layer helpers ported: `bias_htf_4h_layer`, `bias_htf_1d_layer`,
  `trend_dir_layer`, `trend_200_layer`, `macd_sub_layer`, `rsi_sub_layer`,
  `dmi_sub_layer`, `adx_band_layer`, `vwap_sub_layer`, `mtf_dominant_layer`,
  `structure_sub_layer`, `smc_bonus_layer`, `ltf_consensus_layer`,
  `liquidity_sub_layer`, `support_resistance_sub_layer`, `volume_sub_layer`,
  `anti_trap_sub_layer`.
- Old macro helpers removed: `trend_layer`, `momentum_layer`, `volume_layer`,
  `entry_layer`, `anti_trap_layer`, `htf_bias_layer`, `ema_htf_layer`.
- `passes` semantics restored to Pine `longDirectionOK` / `shortDirectionOK`
  shape (`structure_ok && trend_dir && anti_trap && session`).
- 4 inline tests updated to use the new sub-layer helpers
  (`rsi_sub_layer` / `macd_sub_layer` / `dmi_sub_layer` / `adx_band_layer`).

Commit 2 — Penalty stack.
- New helpers `anti_chop_layer`, `strong_conflict_layer`,
  `macro_conflict_layer`, `vol_shock_layer` in `signals.rs`.
- Each profile gained four negative-pass-weight entries:
  `anti_chop = -12`, `strong_conflict = -18`, `macro_conflict = -10`,
  `vol_shock = -4` (Pine V1 lines 723-726 numerics, mirror for BTC V61.9
  lines 624-627). All 5 asset profiles get the same magnitudes per Pine.
- `strong_conflict` and `macro_conflict` are direction-specific (the long
  penalty fires only when the opposing-side conditions are present);
  the helpers take `direction` and apply asymmetrically.

Commit 3 — V61.6 micro-trend override + V61.7 consensus weighting.
- Direction-symmetric *graded* contributions applied directly to the
  accumulator (not via the boolean `add` closure).
- Micro-trend: `+14` matching, `-18` opposing (Pine BTC V61.9 lines
  648-651). Source = `snapshot.micro_trend`.
- Consensus weighting: `+0.22 × consensus_same_dir_score - 0.18 ×
  consensus_opposite_dir_score`. Source = `snapshot.consensus` already
  computed upstream.
- Universal across all 5 asset classes (Pine constants, no per-asset
  overrides needed).

Commit 4 — V63 Gold direction adapter (+ V62 Altcoin analog).
- New `AssetEvaluator::score_adjustments(direction, snapshot) -> f64`
  trait method with `0.0` default for BTC / Forex / IDX.
- Gold override (Pine V1 lines 731-745): LTF-edge weighting (`consensus ×
  0.07 × altLTFWeight` where `altLTFWeight = 0.88`), proxy directional
  weight (`±goldProxyWeight = 9.0`), reversal-OK +8, macro-conflict
  relax (`+8 × 0.18 altHTFRelax`), opposing-proxy −8 penalty.
- Altcoin override (Pine V62 lines 708-718): same shape but
  `altLTFWeight = 1.25`, 0.10/0.08 multiplier pair, +10 reversal-OK,
  no proxy contribution.
- Token-specific Pine signals (`altClean`, `altChaos`,
  `altFakeImpulse`, `liqBullReclaim`) approximated by M15+H1 MTF
  alignment proxies — full token-decomposition refinement still
  tracked under sub-phase 1.7.16 Gap F.

Verification:
- `cargo build` clean after each commit.
- `cargo test --test parity` 5/5 pass with refreshed fixtures after each commit.
- `cargo test --lib` 167/167 pass after each commit (no regressions; one
  pre-existing flaky supervisor test still passes under
  `RUST_TEST_THREADS=1`).
- BTC fixture `bias_text` migrated from `Bias : MAP LONG 75%` (commit 0 vote-
  ratio) → `15%` (commit 0 weighted analog after Gap A revert) → `29%`
  (commit 1 sub-layer skeleton) → `39%` (commits 2+3+4 with penalty stack
  + micro-trend + consensus + V63 adapter applied).

#### Original suggested commit sequencing

**Commit 1 — Sub-layer accumulator skeleton (~3-4h).** Replace
`evaluate_direction`'s macro-layer fold with a per-sub-layer accumulator
struct (e.g. `ScoreBreakdown` with one field per Pine line). Port all 26
Pine sub-layers as boolean predicates against `IndicatorSnapshot`. Keep the
profile weight table for now but make each profile key a *sub-layer* name
(e.g. `bias_htf_4h`, `bias_htf_1d`, `trend_down`, `trend200`, `macd`,
`rsi`, `dmi`, `adx_band`, `vwap`, `mtf_dominant`, `structure_ok`,
`smc_bonus`, ...). Add the failure-penalty path: `add_layer` becomes
`add_layer(name, pass_weight, fail_weight)`. Migrate the Gold profile
table to the new sub-layer keys (~15-20 entries instead of 9 macro keys);
the other 4 asset profiles get default tables too. Land with refreshed
parity fixtures.

**Commit 2 — Penalty stack + Pine `antiChop` / `strongConflict` /
`macroConflict` / `volShock` (~2-3h).** Add four new pass-through helpers
that return `bool` plus a per-direction penalty weight. Wire them into the
accumulator as negative contributions. `antiChop` reuses the existing
sideways regime classifier; `strongConflict` derives from M15 breakout/down
vs H1 pressure; `macroConflict` derives from `bias1w` and `bias1m` MTF
summaries; `volShock` derives from existing `IndicatorSnapshot.volume.z_score`.

**Commit 3 — Micro-trend override (V61.6) + Consensus weighting (V61.7)
(~3-4h).** Add `microBullOverride` / `microBearOverride` helpers driven by
M1+M5 MTF state (current state already lives in `snapshot.mtf`). Add
`consensusLongScore` / `consensusShortScore` derived from M1+M5+M15
unanimity (already partially used by the Altcoin extra_gate). Apply
`+14 / -18` micro-trend contribution and `× 0.22 / × 0.18` consensus
contribution. These are direction-symmetric pairs — the *opposite*
direction's contribution is subtracted.

**Commit 4 — Gold V63 direction adapter (items 17-22) (~2-3h).** Add
`AssetEvaluator::score_adjustments` returning a per-direction
`Vec<(reason, weight)>` for asset-specific contributions. Gold returns
`goldProxyBear × goldProxyWeight` (positive for matching direction,
negative for opposite), `altShortReversalOK +8`, `altChaos / altFakeImpulse
−altTrapPenalty`, and the `useGoldProxyFilter` opposing-bias penalty.
Altcoin V62 (and any future class with a direction adapter) follows the
same pattern.

#### Acceptance criteria

- `cargo run -- export-panel OANDA:XAUUSD 1D` produces a `confidence.short`
  value within ±10 percentage points of Pine on the captured 2026-06-07
  EROPA bar (Pine 100, expected Rust ≥ 90 once all 4 commits land).
- `cargo run -- export-panel <BTC-symbol> 1D` reports an analog score
  that no longer caps trivially at 100 on every clean trend bar (i.e. the
  Pine penalty stack actively pulls the score off the ceiling for
  conflict bars).
- All 5 `tests/fixtures/parity/*.panel.json` snapshots refresh cleanly per
  commit and survive replay (text comparison).
- `cargo test --lib` stays green after each commit. Expect 10-20 score
  threshold adjustments across `signals.rs::tests` because the score
  values change everywhere. `cargo test --test parity` similarly.
- `config/profiles.rs` migrated from 9 macro keys per asset to ~25
  sub-layer keys per asset. Document the per-asset weight tables in a
  comment block at the top of `profiles.rs` so the Pine V1/V62/V58/V5/BTC
  line numbers stay traceable.

#### Risk

Largest blast radius in Phase 1.7 so far. `evaluate_direction` is the
hottest function in the scan loop and its signature is depended on by
every asset evaluator plus the parity test harness. Migration must be
done in 4 ordered commits with parity fixtures refreshed and `cargo test`
green between each — never in one mega-commit.

The score values change for **every** symbol, not just Gold, so the
existing supervisor / scanner / alert / API tests that hard-code
"confidence > 50" or "trade_score >= 70" will need adjusting. Plan for
a triage pass after Commit 1 lands.

### 1.7.18 Per-asset-class Pine-parity rewrite — Drafted 2026-06-09

Sub-phases 1.7.16 + 1.7.17 closed every Pine V1 Gold gap surfaced by the
OANDA:XAUUSD deep-audit (BIAS/CONF analog, RECLAIM source, `altEWFactor`,
SL/TP factors, sub-layer accumulator, penalty stack, V61.6 micro-trend
override, V61.7 consensus weighting, V63 Gold direction adapter). The
shared scaffolding (`AssetEvaluator` trait, `WeightPair` profile table,
sub-layer helpers, `PlanContext` factor fields) now needs equivalent
per-asset wiring for **Altcoin V62**, **BTC V61.9**, **Forex V58**, and
**IDX V5**. `StocksUs` currently delegates to `StocksIdxEvaluator`
(`src/assets/mod.rs:137`); that delegation should be split into its own
evaluator once the IDX one is fully ported (commit 4 below).

Each asset commit follows the same template the Gold work followed:

1. **Panel/render audit** — capture `cargo run -- export-panel <symbol> <tf>`
   output and diff every field against a Pine reference screenshot. File
   the per-field gaps as `Gap A..H` in the same shape as 1.7.16.
2. **Profile sub-layer table calibration** — rebalance the asset's
   `config/profiles.rs` entry so the most-fire layers (always-present
   sub-layers like `trend_dir`, `vwap`, `structure`, `volume`,
   `anti_trap`, `session`) carry enough weight to clear the
   `min_confidence_*` threshold on a clean trend bar, while letting the
   MTF/penalty stack still pull the score around as Pine does.
3. **`AssetEvaluator` overrides** — implement (or refine) the four
   Pine-source-mapped trait methods:
   - `ew_compression_factor` → Pine `altEWFactor`
   - `sl_extension_factor` → Pine `altSLFactor`
   - `tp_compression_factor` → Pine `altTPFactor`
   - `score_adjustments` → Pine V63 direction adapter block
4. **Fixture refresh + regression** — `UPDATE_PARITY_FIXTURES=1 cargo
   test --test parity` + `cargo test --lib` after each commit. Threshold
   tweaks in `tests/parity/*.rs` are allowed when Pine matches the new
   numbers; do not lower a threshold just to make a test pass.

#### Per-asset commit plans

The sub-phases below are independent (each asset can land in isolation)
but share the diff-then-fix workflow. Total scope estimate: **~30-40
hours** across the 4 assets, dominated by the per-asset Pine
read-and-verify pass (~3-4h per asset) and the trait override
implementation (~3-5h per asset).

##### 1.7.18.A — Altcoin V62 rewrite (~6-8h, blast radius: high)

Pine source: `docs/TV_Pine_Scripts/TV_ALTCOIN_V62_0.pine.txt` (1498 lines).
The V62 numerics already partially landed in commits 4 of 1.7.16+1.7.17
(Altcoin overrides of `ew_compression_factor`, `sl_extension_factor`,
`tp_compression_factor`, `score_adjustments`). The remaining V62-specific
work centres on the `altProfile` resolution and token-class signals that
Pine V62 uses but the Rust port currently elides.

**Commit A1 — Altcoin OANDA-style diff capture and gap report.** Pick
2-3 reference candidates (e.g. SOLUSDT, AVAXUSDT, MEMECOINUSDT) that
exercise the Pine `altProfileResolved == "BTC/MAJOR" | "MAJOR ALT" |
"MID ALT" | "MEME"` ladder. Capture Pine reference screenshots from
TradingView. Run `cargo run -- export-panel <symbol> 1D` against the
same closed bar. Diff every panel row, file Gaps A..H as in 1.7.16.

**Commit A2 — `altProfileMult` token resolution (~2h).** Pine V62
line 381: `altProfileMult = altProfileResolved == "BTC/MAJOR" ? 0.70 :
"MAJOR ALT" ? 1.00 : "MID ALT" ? 1.25 : 1.50`. Pine resolves this from
a per-symbol input. Rust currently has no `altProfile` config knob — add
one to `config/default.toml` as
`[[symbols]]` per-entry override with sensible default (`MAJOR ALT`).
Plumb through `SymbolConfig` → `IndicatorSnapshot` → `AssetEvaluator`
overrides. Pin token-class influence to the trait factor methods only;
don't bleed into the additive scorer to keep the accumulator
universal across asset classes.

**Commit A3 — Pine V62 `altClean`/`altChaos`/`altFakeImpulse` signals
(~2h).** These are token-specific candle quality detectors used by
`altShortReversalOK`, `altTrapPenalty`, and the SL `altSLFactor`
wick-buffer branch. Add new helpers in `src/strategy/signals.rs` keyed
off the existing `IndicatorSnapshot.shape.body_ratio`, `clv`, `wick_ratio`,
`volume.session_ratio`, and the new `altProfileMult`. Wire into Altcoin's
`score_adjustments` to refine the LTF-edge weighting trigger.

**Commit A4 — `altMaxChaseATR` no-chase TP gate (~1h).** Pine V62 line
161 caps how far past TP1 the strategy is willing to "chase" before
abstaining. Add `alt_max_chase_atr` to `EntryPlanConfig` (default 0.16
ATR per Pine), surface in `PlanContext`, and reject in
`TakeProfits::from_context` when `tp1 - close > atr * alt_max_chase_atr`.

**Acceptance for 1.7.18.A:**
- Altcoin panel rendering matches Pine reference within ±5pp BIAS/CONF on
  the captured 2-3 reference bars.
- `altProfileMult` resolution is per-symbol-configurable.
- `altClean`/`altChaos` detection has unit-test coverage in
  `assets::altcoin::tests`.
- All 5 parity fixtures refresh cleanly; `cargo test --lib` 167/167
  remains green.

##### 1.7.18.B — BTC V61.9 rewrite (~5-7h, blast radius: medium)

Pine source: `docs/TV_Pine_Scripts/TV_BTC_V61_9_FLOW_INTERPRETATION_PANEL_FULL.pine.txt`
(1429 lines). BTC is the cleanest port because Pine V61.9 has no
`altEWFactor`/`altTPFactor`/`altSLFactor` (those are V63 Gold / V62
Altcoin additions). The BTC accumulator is the **reference** for the
sub-layer skeleton already landed in 1.7.17. The remaining work is
panel-level validation and the V61.6/V61.7 protective stack that lives
in `failReason` but doesn't yet drive the Rust panel.

**Commit B1 — BTC OANDA-style diff capture.** Use BTCUSDT 1D as the
reference symbol (Pine V61.9 is the original BTC script). Capture Pine
panel screenshot, run `cargo run -- export-panel BTCUSDT 1D`, diff every
row. Expect minimal divergence here — the BTC accumulator is already
the closest Rust impl to Pine.

**Commit B2 — `failReason` Pine reason-text port (~3h).** Pine V61.9
line 1028 builds a long disjunctive expression that surfaces the *first*
gating failure to the panel: `flowTrapBlock ? "LOW FLOW WICK TRAP" :
isTrap or cooldownTrap ? "TRAP" : shockActive ? "SHOCK FREEZE" :
planLong and longKill ? "LONG KILL" : ...`. Currently Rust uses
`PanelReport::reason` for free-form telemetry but doesn't follow Pine's
priority ladder. Add a `pine_fail_reason()` helper that mirrors the
Pine expression ordering byte-for-byte, threaded through `PanelInputs`
so the panel renders `reason_text` (new field) alongside `trap_gate_text`.

**Commit B3 — V61.6 `longKill`/`shortKill` deep-add reclaim gate (~2h).**
Pine V61.9 lines 999-1004 introduce `longKill` (true when EW3 broke
and deep-add reclaim window expired) and `shortKill` (mirror). This is
already tracked in Rust as `GuardState.deep_reclaim_bars` but isn't
gating emission. Wire `state.deep_reclaim_active && !reclaimed` into
the emission gate in `evaluate_inner`. Currently the panel shows
`DEEP RECLAIM` cosmetically without blocking — the Pine behaviour is
to hard-block until the reclaim window resets.

**Commit B4 — V61.7 `consensusOK` panel surface (~1h).** Pine
`consensusLongOK = not useConsensusDirection or consensusEdge >=
consensusScoreGap`. Rust already computes `consensus.blocks(...)` and
short-circuits emission but the panel doesn't show the actual edge.
Add `consensus_edge` and `consensus_gap_threshold` to `PanelReport`
so the Telegram/HTML render can show `CONSENSUS: long=+18 vs gap=15`.

**Acceptance for 1.7.18.B:**
- `cargo run -- export-panel BTCUSDT 1D` BIAS/CONF matches Pine within
  ±3pp (BTC has the smallest divergence pre-1.7.18 because the V61.9
  accumulator is already faithfully ported).
- `failReason` text matches Pine's priority ladder on the captured bar.
- Deep-add reclaim hard-blocks emission (verified by a deterministic
  fixture test that breaks EW3 then attempts re-entry mid-reclaim-window).
- Consensus edge surfaces in the panel.

##### 1.7.18.C — Forex V58 rewrite (~4-6h, blast radius: low)

Pine source: `docs/TV_Pine_Scripts/TV_FOREX_V58_TOTAL_PRO.pine.txt`
(442 lines). Forex is the simplest of the four because V58 has no
`altProfile`/proxy/MTF-LTF-edge complications — it's a session-aware
HTF-bias-driven engine. The score range is *also* `[0, 100]`-clamped
but the composition is different from Gold/BTC.

**Commit C1 — Forex diff capture.** Use `EURUSD` and `GBPUSD` 1H as
reference candidates. Capture Pine reference panel from TradingView's
Forex chart. Run `cargo run -- export-panel EURUSD 1H`, diff every row.

**Commit C2 — Port Forex V58 `longScoreRaw`/`shortScoreRaw` (~2h).**
Pine V58 lines 268-289 use a different sub-layer set than V1 Gold /
V61.9 BTC:
- `trendLong` (4-EMA stack: fast > mid > trend), weight 22 — heavier than Gold
- `close > emaTrend` separate weight 8
- `momentumLong` (RSI > 52 AND macdHist > 0 AND macdLine > macdSig), weight 16
- `dmiLong + trendOK` (combined gate), weight 14
- `structureLong` (BoS OR CHoCH OR sweep OR close>refHigh), weight 14
- `pullbackLong OR entryMode == "BREAKOUT"`, weight 8
- `liquidityLong` (sweep OR equalLow), weight 6
- `htfRatio = round(12 * htfLongScore / htfMaxScore)` if present, else 6
- `sessionOK`, weight 6
- `volOK`, weight 4

The Rust Forex profile (`config/profiles.rs:159-181` post-1.7.17)
already has analogous keys but the **weights need recalibration** to
match Pine V58 exactly. Bump `trend_dir` to 22, add a separate
`trend_close_vs_ema200` key at 8, etc.

**Commit C3 — V58 `htfBlockLong`/`htfBlockShort` hard gate (~1h).**
Pine V58 lines 248-249: `htfBlockLong = blockCounterHTF && htfMaxScore
> 0 && htfShortScore == htfMaxScore`. This is a direction-flip kill
gate that's currently approximated as a `confidence` penalty but
should be a hard `passes = false` for the Forex evaluator. Implement
in `forex::ForexEvaluator::extra_gate` returning
`Some("forex_htf_counter_block:long")` per Pine.

**Commit C4 — V58 penalty stack (~2h).** Pine V58 lines 291-294:
```pine
penalty = 0
penalty += chop ? 12 : 0
penalty += atrTooHigh ? 10 : 0
penalty += inRollover and avoidRollover ? 15 : 0
penalty += adx > maxADX ? 8 : 0
```
The Forex penalty stack differs from the universal `anti_chop / strong_conflict
/ macro_conflict / vol_shock` set added in 1.7.17. Add Forex-specific
keys to the profile: `rollover_avoid` (-15), `atr_too_high` (-10),
`adx_overheated` (-8). Implement helpers in `signals.rs` driven by
existing `MarketSession::RolloverAvoid` plus a new
`atr_pct_too_high_layer` (compares ATR/close ratio against
`config.indicators.max_atr_pct`).

**Acceptance for 1.7.18.C:**
- `cargo run -- export-panel EURUSD 1H` BIAS/CONF matches Pine within
  ±3pp. Forex Pine V58 is deterministic so the gap should close
  cleanly once the four commits land.
- Forex `htf_block` flag in `PanelAssetExtras.htf_block_long/short`
  (already exists since 1.7.10) now reflects the Pine hard-block exactly.

##### 1.7.18.D — IDX V5 rewrite + StocksUs split (~7-9h, blast radius: medium-high)

Pine source: `docs/TV_Pine_Scripts/TV_IDX_PRO_5_SAHAM_INDONESIA.pine.txt`
(425 lines). IDX V5 has the most Indonesia-specific signals (IHSG
benchmark, RVOL value-traded gate, manual sentiment knobs, halal screen
considerations). The current Rust port (`src/assets/stocks_idx.rs`)
covers ~60% of these via the soft-layer set. Pine V5 also has a
`downScore` separate from the bull `score` that surfaces a downside-risk
penalty — Rust currently uses `downside_risk_layer` as a single boolean.

**Commit D1 — IDX diff capture.** Pick 2-3 reference IDX symbols across
caps (e.g. BBCA, BBNI, ADRO). Pine V5 reads tick-by-tick IHSG; the Rust
port reads daily IHSG closes from TradingView proxy. Capture Pine + Rust
panels and diff. Expect bigger gaps here than other assets because the
manual sentiment knobs (`manualBigBuy`, `manualRetailSell`,
`manualSpreadPips` proxy concepts) aren't fully wired through
`PanelAssetExtras`.

**Commit D2 — IDX V5 `downScore` separate panel surface (~2h).** Pine
V5 lines 270-289 build `downScore` (range 0..100) from 13 sub-layers
focused on the bearish risk. Currently Rust folds this into one
`downside_risk_layer` boolean. Add `down_score: u32` and
`down_risk_high: bool` to `PanelAssetExtras` so the IDX panel can show
a separate "Downside Risk: 65/100 HIGH" row alongside the bull score.
Hard-gate emission when `down_score >= downRiskLimit` (default 65 per
Pine V5 line 33).

**Commit D3 — IDX V5 score keys rebalance (~2h).** Pine V5 lines 295-317
list 16 positive contributions + 7 penalties. The Rust IDX profile
(post-1.7.17, `config/profiles.rs:202-228`) lists 20 entries but
several mis-mapped:
- `accumulationOK` (Pine: `cmf > 0 && obvSlope > 0 && close >= open`)
  doesn't fully map to Rust's `cmf_obv` layer.
- `breakoutRetestOK` has no Rust equivalent.
- `chaseTooHigh` (`tp1 - close > atr * 0.30`) has no Rust equivalent.
- `manualBigBuy` / `manualRetailSell` need config knobs.

Rebalance the profile entries to align with Pine V5 numerics and add
the missing helpers in `signals.rs`. Manual sentiment knobs become
per-symbol config overrides exposed through `PanelAssetExtras`.

**Commit D4 — Split `StocksUs` evaluator (~2h).** Pine V5 is Indonesia-
specific (IHSG benchmark + Indonesian session windows). `StocksUs`
currently delegates to `StocksIdxEvaluator` which is broken on US
symbols (wrong benchmark, wrong sessions). Create `src/assets/stocks_us.rs`
with a `StocksUsEvaluator` cloned from `StocksIdxEvaluator` but using
SPY/QQQ as the benchmark and US market sessions
(`MarketSession::Usa`). Wire into `assets::evaluator_for`.

Add a `StocksUs` profile entry in `config/profiles.rs` with weights
calibrated for US tech mega-caps (heavier `bias_htf_1d`, lighter
`session` since US stocks trade in a single time zone). Acceptance:
running `cargo run -- export-panel AAPL 1D` no longer fails the
IHSG-benchmark gate.

**Commit D5 — IDX V5 `support`/`resistance` SR-band port (~2h).**
Pine V5 line 165: `resistance = ta.highest(high, srLen)` and
`support = ta.lowest(low, srLen)`. The TP engine uses these as hard
caps. Rust currently has `detect_zones` returning generic
`ZoneKind::Support/Resistance` zones but doesn't use them as TP caps
in the IDX-specific path. Surface IDX-specific zones to
`PlanContext.idx_resistance` and let the IDX evaluator's
`tp_compression_factor` apply the cap.

**Acceptance for 1.7.18.D:**
- `cargo run -- export-panel BBCA 1D` BIAS/CONF + `Downside Risk` panel
  row matches Pine V5 within ±5pp.
- `StocksUs` symbols no longer route through IDX-specific gates
  (verified by a `cargo run -- export-panel AAPL 1D` that emits a
  US-stocks-appropriate panel — no IHSG benchmark, USA session, etc.).
- Pine V5 manual sentiment knobs map to per-symbol config overrides.

#### Cross-asset cleanup commits (~3-5h, after 1.7.18.A-D land)

**Commit X1 — Per-asset `score_adjustments` unit tests (~2h).** The 4
sub-phases above each touch the trait method but the test coverage is
thin. Add `tests/strategy/asset_score_adjustments.rs` with deterministic
snapshot fixtures for each asset+direction pair, asserting the
expected score-adjustment delta. Uses `IndicatorSnapshot::neutral()`
plus per-test field mutation to isolate which Pine clause fires.

**Commit X2 — Per-asset weight-table audit doc (~1h).** Add
`docs/asset_profiles.md` that lists each asset's profile sub-layer
table with the Pine line citation and weight rationale. Keeps the
weight tuning auditable when Pine releases V64 / V63.x updates.

**Commit X3 — Refresh `docs/executive_summary.md` and
`docs/architecture_audit.md` (~1h).** Note the per-asset evaluator
status (Gold ✅, Altcoin ✅+pending refinement, BTC ✅, Forex ✅,
IDX ✅+US-split, StocksUs ✅). Update the "Strategy" row of the exec
summary to reflect all 5+1 asset classes are Pine-parity-aligned.

#### Acceptance for the full 1.7.18 sub-phase

- All 5 evaluators have non-default overrides of
  `ew_compression_factor`, `sl_extension_factor`, `tp_compression_factor`,
  `score_adjustments` matching their respective Pine source.
- `cargo run -- export-panel <symbol> <tf>` produces a panel whose
  BIAS/CONF percentages are within ±5pp of the Pine reference for at
  least 2 captured bars per asset class.
- `cargo test --test parity` 5/5 (or 6/6 if a `stocks_us` suite is
  added) pass with refreshed fixtures after each commit.
- `cargo test --lib` stays green (no regression in the 167-test count;
  expect 10-30 *new* asset-evaluator tests added per the X1 cleanup
  commit).
- `docs/asset_profiles.md` exists and lists each Pine V1/V62/V58/V61.9/V5
  source line per profile entry.

#### Risk and sequencing

- **Independence:** the four asset commits (A/B/C/D) can land in any
  order or even in parallel. Each touches its own `src/assets/<class>.rs`
  + its own `config/profiles.rs` entry. The shared scaffolding
  (`AssetEvaluator` trait, sub-layer helpers) does not need further
  rewrites.
- **Highest-impact first:** if asked to sequence, recommend
  **1.7.18.A (Altcoin)** first — it has the largest user-visible
  divergence on liquid crypto pairs (SOL, AVAX, etc.) where token-class
  signals dominate. Then **1.7.18.D (IDX + US split)** because the
  StocksUs delegation is currently broken. Then **1.7.18.C (Forex)**
  which is small and fast. Then **1.7.18.B (BTC)** which is the smallest
  divergence (V61.9 is already faithfully ported in 1.7.17).
- **Test threshold drift:** as in 1.7.17, expect to revisit the inline
  threshold checks in `src/strategy/signals.rs::tests` for the
  `generates_*_signal_when_all_six_layers_pass` and
  `same_candles_produce_asset_specific_scores` cases. Each asset commit
  may shift `confidence` numerics by 5-15pp.
- **Pine ground-truth dependency:** every commit's acceptance requires
  a Pine reference screenshot from TradingView. The user must supply
  these for the audit step; without them the diff capture is impossible.
- **Backward compatibility:** Pine constants (`altLTFWeight`,
  `goldProxyWeight`, etc.) live in Rust as hard-coded `const f64` for
  now. Future tuning may want to surface these to `config/default.toml`
  under `[assets.<class>]` sections — opportunistic when touching the
  module, not a blocker.

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
