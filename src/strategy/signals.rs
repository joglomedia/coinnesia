use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

use crate::{
    config::{profiles, AppConfig},
    indicators::{
        adx::{calculate_dmi, DmiPoint},
        atr::calculate_atr,
        candle::{analyze, CandleShape},
        liquidity::{detect_liquidity_sweep, SweepKind},
        macd::{calculate_macd, MacdPoint},
        order_block::{detect_order_blocks, OrderBlockKind},
        regime::{classify_regime, MarketRegime, RegimeConfig},
        rsi::calculate_rsi,
        smc::{detect_structure, StructureEvent, StructureState},
        support_resistance::{detect_zones, ZoneKind},
        volume::{volume_engine, VolumePoint},
        vwap::session_vwap,
        IndicatorPoint,
    },
    AssetClass, Candle, Timeframe,
};

use super::{
    confidence::ConfidenceScore,
    entry_plan::{EntryPlan, EntryPlanCalculator},
    guard_state::{latest_swings, GuardAdvanceInput, GuardState},
    mtf::{
        compute_consensus, micro_trend, threshold_for_timeframe, timeframe_set, ConsensusResult,
        MtfCandles, TfSummary,
    },
    plan_context::{classify_flow, FlowState, PlanContext},
    session::{classify_wib, session_allows_asset, MarketSession},
    trap_guard::evaluate_trap_guard,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SignalState {
    Long,
    Short,
    Wait,
    Freeze,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalDirection {
    Long,
    Short,
    Wait,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalResult {
    pub symbol: String,
    pub state: SignalState,
    pub confidence: ConfidenceScore,
    pub reason: String,
    pub entry_plan: Option<EntryPlan>,
}

pub struct SignalGenerator<'a> {
    config: &'a AppConfig,
}

impl<'a> SignalGenerator<'a> {
    pub const fn new(config: &'a AppConfig) -> Self {
        Self { config }
    }

    pub fn evaluate(&self, symbol: &str, candles: &[Candle]) -> SignalResult {
        self.evaluate_inner(symbol, candles, None, &GuardState::default())
            .0
    }

    pub fn evaluate_with_mtf(
        &self,
        symbol: &str,
        candles: &[Candle],
        mtf: &MtfCandles,
    ) -> SignalResult {
        self.evaluate_inner(symbol, candles, Some(mtf), &GuardState::default())
            .0
    }

    /// Full evaluation path that consumes the previous `GuardState` and
    /// returns both the [`SignalResult`] and the new state to persist.
    pub fn evaluate_with_state(
        &self,
        symbol: &str,
        candles: &[Candle],
        mtf: Option<&MtfCandles>,
        prev_state: &GuardState,
    ) -> (SignalResult, GuardState) {
        self.evaluate_inner(symbol, candles, mtf, prev_state)
    }

    fn evaluate_inner(
        &self,
        symbol: &str,
        candles: &[Candle],
        mtf: Option<&MtfCandles>,
        prev_state: &GuardState,
    ) -> (SignalResult, GuardState) {
        if candles.len() < self.config.indicators.atr_length + 1 {
            // No state change without indicator data — return the unchanged state.
            return (
                SignalResult::wait(symbol, "not_enough_candles"),
                prev_state.clone(),
            );
        }

        let Some(symbol_config) = self
            .config
            .symbols
            .iter()
            .find(|item| item.symbol == symbol)
        else {
            return (
                SignalResult::wait(symbol, "symbol_not_configured"),
                prev_state.clone(),
            );
        };
        let timeframe = timeframe_set(&symbol_config.timeframes).primary;
        let snapshot = IndicatorSnapshot::new(candles, self.config, mtf);

        // Compute next guard state from this bar's observations. Done up-front
        // so all downstream short-circuits return a consistent state.
        let (swing_high, swing_low) =
            latest_swings(candles, self.config.strategy.structure_lookback);
        let trap_score_long = evaluate_trap_guard(
            snapshot.candles,
            &self.config.trap_guard,
            SignalDirection::Long,
        )
        .penalty;
        let trap_score_short = evaluate_trap_guard(
            snapshot.candles,
            &self.config.trap_guard,
            SignalDirection::Short,
        )
        .penalty;
        let advance_input = GuardAdvanceInput {
            regime: snapshot.regime,
            trap_score: trap_score_long.max(trap_score_short),
            candles,
            atr: snapshot.atr.value.max(0.0),
            current_swing_high: swing_high,
            current_swing_low: swing_low,
            structure_event: snapshot.structure.event,
            // Deep-add break/reclaim accounting lands with the entry-plan
            // rewrite in 1.7.5; until then both flags stay false and the
            // counter is inert. The state field is still persisted across
            // cycles so the rewrite can wire into it.
            deep_add_broken: false,
            deep_add_reclaimed: false,
        };
        let new_state = prev_state.advance(&self.config.trap_guard, &advance_input);

        // Shock freeze short-circuit comes BEFORE the regime check so we keep
        // returning Frozen even on subsequent bars where the candle is benign.
        if new_state.is_frozen() {
            return (
                SignalResult {
                    symbol: symbol.to_owned(),
                    state: SignalState::Freeze,
                    confidence: ConfidenceScore::neutral(),
                    reason: format!(
                        "shock_freeze remaining={} bars",
                        new_state.shock_freeze_bars
                    ),
                    entry_plan: None,
                },
                new_state,
            );
        }

        let regime = snapshot.regime;
        if regime == MarketRegime::Shock {
            return (SignalResult::freeze(symbol, "shock_regime"), new_state);
        }

        let session = classify_wib(snapshot.latest.ts);
        if !session_allows_asset(session, symbol_config.asset_class) {
            return (
                SignalResult::wait(symbol, format!("session_block:{session:?}")),
                new_state,
            );
        }

        let long = evaluate_direction(
            SignalDirection::Long,
            symbol_config.asset_class,
            &snapshot,
            self.config,
        );
        let short = evaluate_direction(
            SignalDirection::Short,
            symbol_config.asset_class,
            &snapshot,
            self.config,
        );
        let confidence = ConfidenceScore::from_sides(long.score, short.score);
        let threshold = threshold_for_timeframe(timeframe, &self.config.strategy);

        if long.blocks_signal && short.blocks_signal {
            return (
                SignalResult::wait(symbol, "trap_guard_blocked"),
                new_state,
            );
        }

        // Active trap cooldown holds emission Wait while the counter is > 0.
        if new_state.is_trap_cooldown_active() {
            return (
                SignalResult {
                    symbol: symbol.to_owned(),
                    state: SignalState::Wait,
                    confidence,
                    reason: format!(
                        "trap_cooldown remaining={} bars",
                        new_state.trap_cooldown_bars
                    ),
                    entry_plan: None,
                },
                new_state,
            );
        }

        let state = if long.passes && confidence.long >= confidence.short {
            SignalState::Long
        } else if short.passes && confidence.short >= confidence.long {
            SignalState::Short
        } else {
            SignalState::Wait
        };

        // V61.7 consensus gap — block emission when M1/M5/M15 majority opposes.
        if state == SignalState::Long
            && snapshot
                .consensus
                .blocks(SignalDirection::Long, &self.config.strategy.mtf)
        {
            return (
                SignalResult {
                    symbol: symbol.to_owned(),
                    state: SignalState::Wait,
                    confidence,
                    reason: format!(
                        "consensus_block direction=long gap={:.1} threshold={:.1}",
                        snapshot.consensus.gap, self.config.strategy.mtf.consensus_score_gap
                    ),
                    entry_plan: None,
                },
                new_state,
            );
        }
        if state == SignalState::Short
            && snapshot
                .consensus
                .blocks(SignalDirection::Short, &self.config.strategy.mtf)
        {
            return (
                SignalResult {
                    symbol: symbol.to_owned(),
                    state: SignalState::Wait,
                    confidence,
                    reason: format!(
                        "consensus_block direction=short gap={:.1} threshold={:.1}",
                        snapshot.consensus.gap, self.config.strategy.mtf.consensus_score_gap
                    ),
                    entry_plan: None,
                },
                new_state,
            );
        }

        if state == SignalState::Wait
            || !confidence.passes(threshold, self.config.strategy.min_directional_gap)
        {
            return (
                SignalResult {
                    symbol: symbol.to_owned(),
                    state: SignalState::Wait,
                    confidence,
                    reason: format!(
                        "layers_not_met threshold={threshold:.1} long={} short={}",
                        long.reason, short.reason
                    ),
                    entry_plan: None,
                },
                new_state,
            );
        }

        let direction: SignalDirection = state.into();
        let plan_ctx = build_plan_context(
            direction,
            &snapshot,
            session,
            self.config,
            confidence.score(direction),
            trap_score_long.max(trap_score_short),
            &new_state,
        );
        let plan = EntryPlanCalculator::new(&self.config.entry_plan)
            .calculate_from_context(&plan_ctx, snapshot.candles, &self.config.trap_guard);

        if plan.stop_loss.rejected_too_wide {
            return (
                SignalResult {
                    symbol: symbol.to_owned(),
                    state: SignalState::Wait,
                    confidence,
                    reason: format!("sl_too_wide:{:?}", session),
                    entry_plan: Some(plan),
                },
                new_state,
            );
        }

        (
            SignalResult {
                symbol: symbol.to_owned(),
                state,
                confidence,
                reason: format!("six_layer_pass timeframe={timeframe:?} session={session:?}"),
                entry_plan: Some(plan),
            },
            new_state,
        )
    }
}

impl SignalResult {
    pub fn wait(symbol: &str, reason: impl Into<String>) -> Self {
        Self {
            symbol: symbol.to_owned(),
            state: SignalState::Wait,
            confidence: ConfidenceScore::neutral(),
            reason: reason.into(),
            entry_plan: None,
        }
    }

    pub fn freeze(symbol: &str, reason: impl Into<String>) -> Self {
        Self {
            symbol: symbol.to_owned(),
            state: SignalState::Freeze,
            confidence: ConfidenceScore::neutral(),
            reason: reason.into(),
            entry_plan: None,
        }
    }

    pub fn with_entry_plan(mut self, config: &AppConfig, anchor: f64, atr: f64) -> Self {
        self.entry_plan = Some(EntryPlanCalculator::new(&config.entry_plan).calculate(
            self.state.into(),
            anchor,
            atr,
        ));
        self
    }
}

impl From<SignalState> for SignalDirection {
    fn from(value: SignalState) -> Self {
        match value {
            SignalState::Long => Self::Long,
            SignalState::Short => Self::Short,
            SignalState::Wait | SignalState::Freeze => Self::Wait,
        }
    }
}

#[derive(Debug, Clone)]
struct IndicatorSnapshot<'a> {
    candles: &'a [Candle],
    latest: &'a Candle,
    ema_fast: IndicatorPoint,
    ema_slow: IndicatorPoint,
    ema_trend: IndicatorPoint,
    atr: IndicatorPoint,
    rsi: IndicatorPoint,
    dmi: DmiPoint,
    macd: MacdPoint,
    volume: VolumePoint,
    vwap: IndicatorPoint,
    shape: CandleShape,
    structure: StructureState,
    regime: MarketRegime,
    mtf: BTreeMap<Timeframe, TfSummary>,
    consensus: ConsensusResult,
    /// V61.6 microTrend override — exposed for downstream guards (1.7.4 kill
    /// switch, 1.7.6 hard-execution filter). Computed from the lowest available
    /// timeframe's last 5 candles; consumed once those guards land.
    #[allow(dead_code)]
    micro_trend: SignalDirection,
}

impl<'a> IndicatorSnapshot<'a> {
    fn new(candles: &'a [Candle], config: &AppConfig, mtf: Option<&MtfCandles>) -> Self {
        let latest = candles.last().expect("candles checked");
        let closes = candles
            .iter()
            .map(|candle| candle.close)
            .collect::<Vec<_>>();
        let ema_fast = last_ready(&crate::indicators::ema::calculate_ema(
            closes.iter().copied(),
            config.indicators.ema_fast,
        ));
        let ema_slow = last_ready(&crate::indicators::ema::calculate_ema(
            closes.iter().copied(),
            config.indicators.ema_slow,
        ));
        let ema_trend = last_ready(&crate::indicators::ema::calculate_ema(
            closes.iter().copied(),
            config.indicators.ema_trend,
        ));
        let atr = last_ready(&calculate_atr(candles, config.indicators.atr_length));
        let rsi = last_ready(&calculate_rsi(&closes, config.indicators.rsi_length));
        let dmi = calculate_dmi(
            candles,
            config.indicators.adx_length,
            config.indicators.adx_smoothing,
        )
        .last()
        .copied()
        .unwrap_or_else(pending_dmi);
        let macd = calculate_macd(
            &closes,
            config.indicators.macd_fast,
            config.indicators.macd_slow,
            config.indicators.macd_signal,
        )
        .last()
        .copied()
        .unwrap_or_else(pending_macd);
        let volume = volume_engine(candles, config.indicators.volume_ma_length)
            .last()
            .copied()
            .unwrap_or_else(pending_volume);
        let vwap = last_ready(&session_vwap(candles));
        let shape = analyze(latest);
        let structure = detect_structure(
            candles,
            config.strategy.structure_lookback,
            config.strategy.min_structure_score,
        );
        let regime = classify_regime(
            candles,
            &calculate_dmi(
                candles,
                config.indicators.adx_length,
                config.indicators.adx_smoothing,
            ),
            &calculate_rsi(&closes, config.indicators.rsi_length),
            RegimeConfig::default(),
        );

        let mut summaries: BTreeMap<Timeframe, TfSummary> = BTreeMap::new();
        if let Some(mtf) = mtf {
            for (tf, series) in &mtf.by_tf {
                if series.is_empty() {
                    summaries.insert(*tf, TfSummary::neutral(*tf));
                    continue;
                }
                let summary = TfSummary::from_candles(
                    *tf,
                    series,
                    config.indicators.adx_length,
                    config.indicators.adx_smoothing,
                    config.indicators.ema_trend,
                    config.strategy.structure_lookback,
                    config.strategy.min_structure_score,
                );
                summaries.insert(*tf, summary);
            }
        }
        let consensus = compute_consensus(&summaries);
        let lowest_ltf_candles = mtf
            .and_then(|m| {
                [Timeframe::M1, Timeframe::M5, Timeframe::M15]
                    .iter()
                    .find_map(|tf| {
                        let series = m.candles(*tf);
                        if series.is_empty() {
                            None
                        } else {
                            Some(series)
                        }
                    })
            })
            .unwrap_or(candles);
        let micro_trend_dir = micro_trend(lowest_ltf_candles, &config.strategy.mtf);

        Self {
            candles,
            latest,
            ema_fast,
            ema_slow,
            ema_trend,
            atr,
            rsi,
            dmi,
            macd,
            volume,
            vwap,
            shape,
            structure,
            regime,
            mtf: summaries,
            consensus,
            micro_trend: micro_trend_dir,
        }
    }
}

#[derive(Debug, Clone)]
struct DirectionEvaluation {
    score: f64,
    passes: bool,
    blocks_signal: bool,
    reason: String,
}

fn evaluate_direction(
    direction: SignalDirection,
    asset_class: AssetClass,
    snapshot: &IndicatorSnapshot<'_>,
    config: &AppConfig,
) -> DirectionEvaluation {
    let profile_map = profiles::default_profiles();
    let profile = profile_map.get(&asset_class);
    let mut score = 0.0;
    let mut missing = Vec::new();

    let trend = trend_layer(direction, snapshot);
    let momentum = momentum_layer(direction, snapshot);
    let volume = volume_layer(direction, snapshot);
    let entry = entry_layer(direction, snapshot, config);
    let anti_trap = anti_trap_layer(direction, snapshot, config);
    let regime_session = snapshot.regime.allows_signals();
    let htf_bias = htf_bias_layer(direction, snapshot);
    let ema_htf = ema_htf_layer(direction, snapshot);

    add_layer(
        &mut score,
        profile,
        &["structure", "ema"],
        trend,
        &mut missing,
        "trend",
    );
    add_layer(
        &mut score,
        profile,
        &["htf_bias"],
        htf_bias,
        &mut missing,
        "htf_bias",
    );
    add_layer(
        &mut score,
        profile,
        &["ema_htf"],
        ema_htf,
        &mut missing,
        "ema_htf",
    );
    add_layer(
        &mut score,
        profile,
        &["rsi", "macd", "momentum", "adx", "adx_dmi"],
        momentum,
        &mut missing,
        "momentum",
    );
    add_layer(
        &mut score,
        profile,
        &["volume", "token_volume", "tick_volume", "rvol_value_gate"],
        volume,
        &mut missing,
        "volume",
    );
    add_layer(
        &mut score,
        profile,
        &["liquidity", "support_resistance", "vwap", "ltf_consensus"],
        entry,
        &mut missing,
        "entry",
    );
    add_layer(
        &mut score,
        profile,
        &["wick_chaos", "downside_risk", "atr_expansion", "atr_news"],
        anti_trap,
        &mut missing,
        "anti_trap",
    );
    add_layer(
        &mut score,
        profile,
        &["session"],
        regime_session,
        &mut missing,
        "regime_session",
    );

    let trap = evaluate_trap_guard(snapshot.candles, &config.trap_guard, direction);
    score = (score - trap.penalty).clamp(0.0, 100.0);
    DirectionEvaluation {
        score,
        passes: trend && momentum && volume && entry && anti_trap && regime_session,
        blocks_signal: trap.blocks_signal,
        reason: if missing.is_empty() {
            "all_layers_pass".to_owned()
        } else {
            missing.join("|")
        },
    }
}

fn add_layer(
    score: &mut f64,
    profile: Option<&profiles::AssetProfile>,
    keys: &[&str],
    passed: bool,
    missing: &mut Vec<&'static str>,
    name: &'static str,
) {
    if passed {
        let weight = profile
            .map(|profile| keys.iter().filter_map(|key| profile.weights.get(key)).sum())
            .unwrap_or(0.0);
        *score += weight;
    } else {
        missing.push(name);
    }
}

/// V61.7 HTF bias check. Returns the configured profile weight when the
/// preferred HTF (D1 → H4 → H1, whichever is available) agrees with the
/// proposed direction. When no MTF data is configured (legacy single-timeframe
/// symbols) or the chosen TF has no readable bias yet, the layer passes
/// through so it does not penalize the score.
fn htf_bias_layer(direction: SignalDirection, snapshot: &IndicatorSnapshot<'_>) -> bool {
    if snapshot.mtf.is_empty() {
        return true;
    }
    let summary = snapshot
        .mtf
        .get(&Timeframe::D1)
        .or_else(|| snapshot.mtf.get(&Timeframe::H4))
        .or_else(|| snapshot.mtf.get(&Timeframe::H1));
    let Some(summary) = summary else {
        return true;
    };
    match (direction, summary.bias) {
        (SignalDirection::Long, SignalDirection::Long) => true,
        (SignalDirection::Short, SignalDirection::Short) => true,
        (_, SignalDirection::Wait) => true,
        _ => false,
    }
}

/// Companion to `htf_bias_layer` that checks the close-vs-EMA-200 alignment
/// on the chosen HTF. Same pass-through fallback semantics.
fn ema_htf_layer(direction: SignalDirection, snapshot: &IndicatorSnapshot<'_>) -> bool {
    if snapshot.mtf.is_empty() {
        return true;
    }
    let summary = snapshot
        .mtf
        .get(&Timeframe::D1)
        .or_else(|| snapshot.mtf.get(&Timeframe::H4));
    let Some(summary) = summary else {
        return true;
    };
    match (direction, summary.ema_trend_above) {
        (SignalDirection::Long, Some(true)) => true,
        (SignalDirection::Short, Some(false)) => true,
        (_, None) => true,
        _ => false,
    }
}

fn trend_layer(direction: SignalDirection, snapshot: &IndicatorSnapshot<'_>) -> bool {
    let structure = matches!(
        (direction, snapshot.structure.event),
        (
            SignalDirection::Long,
            StructureEvent::BullishBos | StructureEvent::BullishChoch
        ) | (
            SignalDirection::Short,
            StructureEvent::BearishBos | StructureEvent::BearishChoch
        )
    );
    let ema = match direction {
        SignalDirection::Long => {
            (!snapshot.ema_fast.ready || snapshot.latest.close > snapshot.ema_fast.value)
                && (!snapshot.ema_slow.ready || snapshot.latest.close > snapshot.ema_slow.value)
                && (!snapshot.ema_trend.ready || snapshot.latest.close > snapshot.ema_trend.value)
        }
        SignalDirection::Short => {
            (!snapshot.ema_fast.ready || snapshot.latest.close < snapshot.ema_fast.value)
                && (!snapshot.ema_slow.ready || snapshot.latest.close < snapshot.ema_slow.value)
                && (!snapshot.ema_trend.ready || snapshot.latest.close < snapshot.ema_trend.value)
        }
        SignalDirection::Wait => false,
    };
    structure && ema
}

fn momentum_layer(direction: SignalDirection, snapshot: &IndicatorSnapshot<'_>) -> bool {
    let adx_active = snapshot.dmi.adx.ready && snapshot.dmi.adx.value > 16.0;
    let dmi = match direction {
        SignalDirection::Long => {
            snapshot.dmi.di_plus.ready
                && snapshot.dmi.di_minus.ready
                && snapshot.dmi.di_plus.value > snapshot.dmi.di_minus.value
        }
        SignalDirection::Short => {
            snapshot.dmi.di_plus.ready
                && snapshot.dmi.di_minus.ready
                && snapshot.dmi.di_minus.value > snapshot.dmi.di_plus.value
        }
        SignalDirection::Wait => false,
    };
    let rsi = match direction {
        SignalDirection::Long => {
            snapshot.rsi.ready && (50.0..=72.0).contains(&snapshot.rsi.value)
        }
        SignalDirection::Short => {
            snapshot.rsi.ready && (28.0..=50.0).contains(&snapshot.rsi.value)
        }
        SignalDirection::Wait => false,
    };
    let macd = match direction {
        SignalDirection::Long => {
            snapshot.macd.macd.ready
                && snapshot.macd.signal.ready
                && snapshot.macd.histogram.ready
                && snapshot.macd.macd.value > snapshot.macd.signal.value
                && snapshot.macd.histogram.value > 0.0
        }
        SignalDirection::Short => {
            snapshot.macd.macd.ready
                && snapshot.macd.signal.ready
                && snapshot.macd.histogram.ready
                && snapshot.macd.macd.value < snapshot.macd.signal.value
                && snapshot.macd.histogram.value < 0.0
        }
        SignalDirection::Wait => false,
    };

    adx_active && dmi && (rsi || macd)
}

fn volume_layer(direction: SignalDirection, snapshot: &IndicatorSnapshot<'_>) -> bool {
    let flow = snapshot.volume.session_ratio.ready
        && snapshot.volume.session_ratio.value >= 1.15
        && !snapshot.volume.decaying;
    let clv = match direction {
        SignalDirection::Long => snapshot.shape.close_location_value > 0.0,
        SignalDirection::Short => snapshot.shape.close_location_value < 0.0,
        SignalDirection::Wait => false,
    };
    flow && clv
}

fn entry_layer(
    direction: SignalDirection,
    snapshot: &IndicatorSnapshot<'_>,
    config: &AppConfig,
) -> bool {
    let atr_tolerance = snapshot.atr.value.max(0.0) * 0.08;
    let sweep = detect_liquidity_sweep(
        snapshot.candles,
        20.min(snapshot.candles.len() - 1),
        atr_tolerance,
    );
    let order_blocks = detect_order_blocks(
        snapshot.candles,
        20,
        config.indicators.ob_displacement_atr,
    );
    let zones = detect_zones(
        snapshot.candles,
        30,
        snapshot.atr.value.max(0.0) * config.indicators.sr_cluster_atr,
        6,
    );

    let structure_entry = matches!(
        (direction, snapshot.structure.event),
        (
            SignalDirection::Long,
            StructureEvent::BullishBos | StructureEvent::BullishChoch
        ) | (
            SignalDirection::Short,
            StructureEvent::BearishBos | StructureEvent::BearishChoch
        )
    );
    let vwap_entry = match direction {
        SignalDirection::Long => {
            snapshot.vwap.ready && snapshot.latest.close >= snapshot.vwap.value
        }
        SignalDirection::Short => {
            snapshot.vwap.ready && snapshot.latest.close <= snapshot.vwap.value
        }
        SignalDirection::Wait => false,
    };
    let sweep_entry = matches!(
        (direction, sweep),
        (SignalDirection::Long, Some(sweep)) if sweep.kind == SweepKind::Low && sweep.reclaimed
    ) || matches!(
        (direction, sweep),
        (SignalDirection::Short, Some(sweep)) if sweep.kind == SweepKind::High && sweep.reclaimed
    );
    let order_block_entry = order_blocks.iter().any(|block| {
        block.valid
            && matches!(
                (direction, block.kind),
                (SignalDirection::Long, OrderBlockKind::Bullish)
                    | (SignalDirection::Short, OrderBlockKind::Bearish)
            )
    });
    let sr_entry = match direction {
        SignalDirection::Long => zones.iter().any(|zone| zone.kind == ZoneKind::Support),
        SignalDirection::Short => zones.iter().any(|zone| zone.kind == ZoneKind::Resistance),
        SignalDirection::Wait => false,
    };

    structure_entry || (vwap_entry && (sweep_entry || order_block_entry || sr_entry))
}

fn anti_trap_layer(
    direction: SignalDirection,
    snapshot: &IndicatorSnapshot<'_>,
    config: &AppConfig,
) -> bool {
    if snapshot.shape.trap_risk && snapshot.shape.body_ratio < 0.35 {
        return false;
    }
    !evaluate_trap_guard(snapshot.candles, &config.trap_guard, direction).blocks_signal
}

fn last_ready(points: &[IndicatorPoint]) -> IndicatorPoint {
    points
        .iter()
        .rev()
        .find(|point| point.ready)
        .copied()
        .unwrap_or_else(IndicatorPoint::pending)
}

fn build_plan_context(
    direction: SignalDirection,
    snapshot: &IndicatorSnapshot<'_>,
    session: MarketSession,
    config: &AppConfig,
    confidence: f64,
    trap_score: f64,
    state: &GuardState,
) -> PlanContext {
    let latest = snapshot.latest;
    let atr = snapshot.atr.value.max(0.0);
    let candles = snapshot.candles;

    // Daily / weekly H-L approximations. When a real D1/W1 timeframe is
    // available via MTF the upstream snapshot uses it; here we fall back to
    // a rolling lookback of the primary series. Using 24 bars ≈ "yesterday's
    // window" for intraday and the most recent bar for daily/weekly.
    let (daily_high, daily_low) = recent_high_low(candles, 24);
    let (weekly_high, weekly_low) = recent_high_low(candles, 120);
    let (sw_high, sw_low) = recent_high_low(candles, config.strategy.structure_lookback.max(8));

    // V61.8 flow classification. session_ratio + ATR ratio against an SMA
    // baseline of recent ATR. Fall back to Mid when readings aren't ready.
    let flow_state = if snapshot.volume.session_ratio.ready && atr > 0.0 {
        let baseline_atr = average_atr(candles, config.entry_plan.flow.flow_lookback);
        let atr_ratio = if baseline_atr > 0.0 {
            atr / baseline_atr
        } else {
            1.0
        };
        classify_flow(
            snapshot.volume.session_ratio.value,
            atr_ratio,
            snapshot.dmi.adx.value,
            config.entry_plan.flow.low_flow_vol_ratio,
            config.entry_plan.flow.low_flow_atr_ratio,
            config.entry_plan.flow.high_flow_vol_ratio,
            config.entry_plan.flow.high_flow_atr_ratio,
        )
    } else {
        FlowState::Mid
    };

    let vol_shock = snapshot
        .volume
        .z_score
        .ready
        .then(|| snapshot.volume.z_score.value >= 3.0)
        .unwrap_or(false);

    PlanContext {
        direction,
        close: latest.close,
        atr,
        adx: if snapshot.dmi.adx.ready {
            snapshot.dmi.adx.value
        } else {
            0.0
        },
        confidence,
        trap_score,
        upper_wick: snapshot.shape.upper_wick,
        lower_wick: snapshot.shape.lower_wick,
        swing_high: sw_high,
        swing_low: sw_low,
        daily_high,
        daily_low,
        weekly_high,
        weekly_low,
        vwap: if snapshot.vwap.ready {
            snapshot.vwap.value
        } else {
            latest.close
        },
        ema_fast: if snapshot.ema_fast.ready {
            snapshot.ema_fast.value
        } else {
            latest.close
        },
        ema_slow: if snapshot.ema_slow.ready {
            snapshot.ema_slow.value
        } else {
            latest.close
        },
        session,
        regime: snapshot.regime,
        flow_state,
        trap_now: trap_score >= config.trap_guard.trap_score_threshold,
        cooldown_active: state.is_trap_cooldown_active(),
        vol_shock,
        shock_active: state.is_frozen() || snapshot.regime == MarketRegime::Shock,
    }
}

fn recent_high_low(candles: &[Candle], lookback: usize) -> (f64, f64) {
    if candles.is_empty() {
        return (0.0, 0.0);
    }
    let start = candles.len().saturating_sub(lookback.max(1));
    let window = &candles[start..];
    let high = window
        .iter()
        .map(|c| c.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let low = window.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
    (high, low)
}

fn average_atr(candles: &[Candle], lookback: usize) -> f64 {
    if candles.is_empty() {
        return 0.0;
    }
    let start = candles.len().saturating_sub(lookback.max(1));
    let window = &candles[start..];
    let sum: f64 = window.iter().map(|c| (c.high - c.low).max(0.0)).sum();
    sum / window.len() as f64
}

fn pending_dmi() -> DmiPoint {
    DmiPoint {
        adx: IndicatorPoint::pending(),
        di_plus: IndicatorPoint::pending(),
        di_minus: IndicatorPoint::pending(),
    }
}

fn pending_macd() -> MacdPoint {
    MacdPoint {
        macd: IndicatorPoint::pending(),
        signal: IndicatorPoint::pending(),
        histogram: IndicatorPoint::pending(),
    }
}

fn pending_volume() -> VolumePoint {
    VolumePoint {
        volume: 0.0,
        average: IndicatorPoint::pending(),
        ratio: IndicatorPoint::pending(),
        z_score: IndicatorPoint::pending(),
        session_average: IndicatorPoint::pending(),
        session_ratio: IndicatorPoint::pending(),
        decaying: false,
        pressure: crate::indicators::volume::VolumePressure::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};

    use super::*;

    #[test]
    fn generates_long_signal_when_all_six_layers_pass() {
        let mut config = test_config();
        config.symbols[0].timeframes = vec!["1d".to_owned()];
        let candles = trending_candles(80, SignalDirection::Long);
        let signal = SignalGenerator::new(&config).evaluate("BTCUSDT", &candles);

        assert_eq!(signal.state, SignalState::Long);
        assert!(signal.confidence.long >= config.strategy.min_confidence_1d);
        assert!(signal.entry_plan.is_some());
    }

    #[test]
    fn generates_short_signal_when_all_six_layers_pass() {
        let mut config = test_config();
        config.symbols[0].timeframes = vec!["1d".to_owned()];
        let candles = trending_candles(80, SignalDirection::Short);
        let signal = SignalGenerator::new(&config).evaluate("BTCUSDT", &candles);

        assert_eq!(signal.state, SignalState::Short);
        assert!(signal.confidence.short >= config.strategy.min_confidence_1d);
        assert!(signal.entry_plan.is_some());
    }

    #[test]
    fn returns_wait_when_session_gate_blocks_asset() {
        let mut config = test_config();
        config.symbols[0].asset_class = AssetClass::Forex;
        config.symbols[0].timeframes = vec!["1d".to_owned()];
        let candles = asia_session_candles();
        let signal = SignalGenerator::new(&config).evaluate("BTCUSDT", &candles);

        assert_eq!(signal.state, SignalState::Wait);
        assert!(signal.reason.starts_with("session_block"));
    }

    #[test]
    fn returns_freeze_when_shock_regime_is_detected() {
        let config = test_config();
        let mut candles = trending_candles(40, SignalDirection::Long);
        candles.push(Candle {
            ts: candles.last().unwrap().ts + Duration::days(1),
            open: 150.0,
            high: 220.0,
            low: 80.0,
            close: 180.0,
            volume: 10_000.0,
        });
        let signal = SignalGenerator::new(&config).evaluate("BTCUSDT", &candles);

        assert_eq!(signal.state, SignalState::Freeze);
    }

    #[test]
    fn same_candles_produce_asset_specific_scores() {
        let mut btc = test_config();
        btc.symbols[0].asset_class = AssetClass::Btc;
        btc.symbols[0].timeframes = vec!["1d".to_owned()];
        let mut alt = btc.clone();
        alt.symbols[0].asset_class = AssetClass::Gold;
        let candles = trending_candles(80, SignalDirection::Long);

        let btc_signal = SignalGenerator::new(&btc).evaluate("BTCUSDT", &candles);
        let alt_signal = SignalGenerator::new(&alt).evaluate("BTCUSDT", &candles);

        assert_ne!(btc_signal.confidence.long, alt_signal.confidence.long);
    }

    #[test]
    fn sideways_regime_does_not_auto_block_emission() {
        // Pre-fix: signals.rs short-circuited to Wait with reason="regime_block:Sideways"
        // for any non-TrendExpansion regime. Pine semantics: only Shock blocks; let
        // scoring + directional gap decide for everything else.
        let config = test_config();
        let candles = flat_candles(80);
        let signal = SignalGenerator::new(&config).evaluate("BTCUSDT", &candles);

        assert_ne!(signal.state, SignalState::Freeze);
        assert!(
            !signal.reason.starts_with("regime_block"),
            "regime_block short-circuit must be gone; got reason: {}",
            signal.reason
        );
    }

    #[test]
    fn strict_rsi_layer_rejects_overbought_long() {
        let candles = flat_candles(25);
        let config = test_config();
        let mut snapshot = IndicatorSnapshot::new(&candles, &config, None);
        snapshot.rsi = IndicatorPoint::ready(80.0);
        snapshot.macd.macd = IndicatorPoint::ready(1.0);
        snapshot.macd.signal = IndicatorPoint::ready(1.5);
        snapshot.macd.histogram = IndicatorPoint::ready(-0.5);
        snapshot.dmi.adx = IndicatorPoint::ready(25.0);
        snapshot.dmi.di_plus = IndicatorPoint::ready(30.0);
        snapshot.dmi.di_minus = IndicatorPoint::ready(15.0);

        assert!(
            !momentum_layer(SignalDirection::Long, &snapshot),
            "RSI=80 (overbought) + bearish MACD must fail strict momentum layer"
        );
    }

    #[test]
    fn strict_rsi_layer_accepts_in_range_long() {
        let candles = flat_candles(25);
        let config = test_config();
        let mut snapshot = IndicatorSnapshot::new(&candles, &config, None);
        snapshot.rsi = IndicatorPoint::ready(60.0);
        snapshot.macd.macd = IndicatorPoint::ready(1.0);
        snapshot.macd.signal = IndicatorPoint::ready(0.5);
        snapshot.macd.histogram = IndicatorPoint::ready(0.5);
        snapshot.dmi.adx = IndicatorPoint::ready(25.0);
        snapshot.dmi.di_plus = IndicatorPoint::ready(30.0);
        snapshot.dmi.di_minus = IndicatorPoint::ready(15.0);

        assert!(
            momentum_layer(SignalDirection::Long, &snapshot),
            "RSI=60 (in 50..=72) + bullish MACD must pass strict momentum layer"
        );
    }

    #[test]
    fn strict_macd_layer_requires_positive_histogram_for_long() {
        let candles = flat_candles(25);
        let config = test_config();
        let mut snapshot = IndicatorSnapshot::new(&candles, &config, None);
        // RSI out of range so OR clause cannot rescue MACD failure.
        snapshot.rsi = IndicatorPoint::ready(40.0);
        // macd > signal (would pass the pre-fix `>=` check)…
        snapshot.macd.macd = IndicatorPoint::ready(1.0);
        snapshot.macd.signal = IndicatorPoint::ready(0.5);
        // …but histogram is negative — strict gate must reject.
        snapshot.macd.histogram = IndicatorPoint::ready(-0.2);
        snapshot.dmi.adx = IndicatorPoint::ready(25.0);
        snapshot.dmi.di_plus = IndicatorPoint::ready(30.0);
        snapshot.dmi.di_minus = IndicatorPoint::ready(15.0);

        assert!(
            !momentum_layer(SignalDirection::Long, &snapshot),
            "negative MACD histogram must fail strict momentum layer even if macd>signal"
        );
    }

    #[test]
    fn strict_macd_layer_requires_negative_histogram_for_short() {
        let candles = flat_candles(25);
        let config = test_config();
        let mut snapshot = IndicatorSnapshot::new(&candles, &config, None);
        snapshot.rsi = IndicatorPoint::ready(60.0); // out of short range 28..=50
        snapshot.macd.macd = IndicatorPoint::ready(0.5);
        snapshot.macd.signal = IndicatorPoint::ready(1.0);
        snapshot.macd.histogram = IndicatorPoint::ready(0.2); // wrong sign
        snapshot.dmi.adx = IndicatorPoint::ready(25.0);
        snapshot.dmi.di_plus = IndicatorPoint::ready(15.0);
        snapshot.dmi.di_minus = IndicatorPoint::ready(30.0);

        assert!(
            !momentum_layer(SignalDirection::Short, &snapshot),
            "positive MACD histogram must fail short strict momentum layer"
        );
    }

    #[test]
    fn snapshot_carries_h4_and_d1_bias_from_long_fixture() {
        // Acceptance for 1.7.3: snapshot includes non-null H4/D1 bias on a
        // fixture chain that spans ≥ 200 D1 bars.
        let d1_candles = trending_candles(220, SignalDirection::Long);
        let mut h4_candles = Vec::with_capacity(d1_candles.len() * 6);
        for c in &d1_candles {
            for slot in 0..6 {
                h4_candles.push(Candle {
                    ts: c.ts + Duration::hours(slot * 4),
                    open: c.open,
                    high: c.high,
                    low: c.low,
                    close: c.close,
                    volume: c.volume / 6.0,
                });
            }
        }
        let mut by_tf = BTreeMap::new();
        by_tf.insert(Timeframe::D1, d1_candles.clone());
        by_tf.insert(Timeframe::H4, h4_candles);
        let mtf = MtfCandles::new(Timeframe::D1, by_tf);
        let config = test_config();
        let snapshot = IndicatorSnapshot::new(&d1_candles, &config, Some(&mtf));

        let d1 = snapshot.mtf.get(&Timeframe::D1).expect("D1 summary present");
        let h4 = snapshot.mtf.get(&Timeframe::H4).expect("H4 summary present");
        assert_eq!(d1.bias, SignalDirection::Long);
        assert_eq!(h4.bias, SignalDirection::Long);
        assert!(d1.ema_trend_above == Some(true));
    }

    #[test]
    fn shock_freeze_counter_blocks_emission_for_configured_bars() {
        // Acceptance for 1.7.4: after a shock candle, the next
        // `shock_freeze_bars` evaluations must return Freeze even on benign
        // candles. The bar that follows must no longer freeze.
        let mut config = test_config();
        config.symbols[0].timeframes = vec!["1d".to_owned()];
        let freeze_window = config.trap_guard.shock_freeze_bars;

        let benign_candles = trending_candles(40, SignalDirection::Long);
        let mut shock_candles = benign_candles.clone();
        // Add a single shock bar at the end.
        let last_ts = shock_candles.last().unwrap().ts;
        shock_candles.push(Candle {
            ts: last_ts + Duration::days(1),
            open: 150.0,
            high: 220.0,
            low: 80.0,
            close: 180.0,
            volume: 10_000.0,
        });

        let generator = SignalGenerator::new(&config);
        let mut state = super::super::guard_state::GuardState::default();

        // Cycle 1: shock candle arms the freeze counter.
        let (signal, next) =
            generator.evaluate_with_state("BTCUSDT", &shock_candles, None, &state);
        assert_eq!(signal.state, SignalState::Freeze);
        assert_eq!(next.shock_freeze_bars, freeze_window);
        state = next;

        // Subsequent cycles on benign candles must keep returning Freeze
        // until the counter hits zero. The shock bar itself already counted as
        // the first frozen bar, so we expect (shock_freeze_bars - 1) more.
        for bar in 0..(freeze_window - 1) {
            let (signal, next) =
                generator.evaluate_with_state("BTCUSDT", &benign_candles, None, &state);
            assert_eq!(
                signal.state,
                SignalState::Freeze,
                "bar {bar} must still be Frozen (remaining={})",
                next.shock_freeze_bars
            );
            state = next;
        }

        // After exactly `freeze_window` decrement bars, the next evaluation
        // must no longer freeze.
        let (signal, _) =
            generator.evaluate_with_state("BTCUSDT", &benign_candles, None, &state);
        assert_ne!(
            signal.state,
            SignalState::Freeze,
            "after shock_freeze_bars exhaustion the gate must release"
        );
    }

    #[test]
    fn consensus_block_short_circuits_when_ltf_triad_opposes_long() {
        // Acceptance for 1.7.3: consensus score blocks emission when M1/M5/M15
        // disagree with the candidate side by more than `consensus_score_gap`.
        let mut config = test_config();
        config.symbols[0].timeframes = vec!["1d".to_owned()];
        let primary = trending_candles(80, SignalDirection::Long);

        // Build M1/M5/M15 candle series that show the opposite (Short) bias.
        let bearish_ltf = trending_candles(120, SignalDirection::Short);
        let mut by_tf = BTreeMap::new();
        by_tf.insert(Timeframe::D1, primary.clone());
        by_tf.insert(Timeframe::M1, bearish_ltf.clone());
        by_tf.insert(Timeframe::M5, bearish_ltf.clone());
        by_tf.insert(Timeframe::M15, bearish_ltf);
        let mtf = MtfCandles::new(Timeframe::D1, by_tf);
        let signal =
            SignalGenerator::new(&config).evaluate_with_mtf("BTCUSDT", &primary, &mtf);

        assert_eq!(signal.state, SignalState::Wait);
        assert!(
            signal.reason.contains("consensus_block"),
            "expected consensus_block reason, got: {}",
            signal.reason
        );
    }

    fn test_config() -> AppConfig {
        let mut config = AppConfig::from_default_toml().expect("default config parses");
        config.strategy.min_confidence_15m = 45.0;
        config.strategy.min_confidence_1h = 45.0;
        config.strategy.min_confidence_4h = 45.0;
        config.strategy.min_confidence_1d = 45.0;
        config.strategy.min_directional_gap = 5.0;
        config.strategy.min_structure_score = 5.0;
        // The synthetic `trending_candles` fixture is a strict monotone slope
        // with no pullbacks, so swingLow / swingHigh sit several ATR away from
        // close. Pine itself would reject these setups under V61.6 dynamic
        // max-SL caps; raise the caps here so the layer-pass tests still
        // exercise the emission path. Real-market candles produce tighter
        // swings well under the production caps.
        config.entry_plan.max_sl_asia_atr = 12.0;
        config.entry_plan.max_sl_europe_atr = 12.0;
        config.entry_plan.max_sl_usa_atr = 12.0;
        config
    }

    fn flat_candles(len: usize) -> Vec<Candle> {
        let start = Utc.with_ymd_and_hms(2026, 5, 20, 14, 0, 0).unwrap();
        (0..len)
            .map(|idx| Candle {
                ts: start + Duration::days(idx as i64),
                open: 100.0,
                high: 100.5,
                low: 99.5,
                close: 100.0,
                volume: 1_000.0,
            })
            .collect()
    }

    fn asia_session_candles() -> Vec<Candle> {
        let mut candles = trending_candles(80, SignalDirection::Long);
        for (idx, candle) in candles.iter_mut().enumerate() {
            candle.ts =
                Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap() + Duration::minutes(idx as i64);
        }
        candles
    }

    fn trending_candles(len: usize, direction: SignalDirection) -> Vec<Candle> {
        let start = Utc.with_ymd_and_hms(2026, 5, 20, 14, 0, 0).unwrap();
        let mut candles = Vec::with_capacity(len);
        for idx in 0..len {
            let trend = idx as f64 * 1.2;
            let base = match direction {
                SignalDirection::Long => 100.0 + trend,
                SignalDirection::Short => 200.0 - trend,
                SignalDirection::Wait => 100.0,
            };
            let breakout = idx + 1 == len;
            let (open, close, high, low) = match direction {
                SignalDirection::Long => {
                    let open = base;
                    let close = base + if breakout { 6.0 } else { 1.0 };
                    (open, close, close + 1.0, open - 1.0)
                }
                SignalDirection::Short => {
                    let open = base;
                    let close = base - if breakout { 6.0 } else { 1.0 };
                    (open, close, open + 1.0, close - 1.0)
                }
                SignalDirection::Wait => (base, base, base + 1.0, base - 1.0),
            };
            candles.push(Candle {
                ts: start + Duration::days(idx as i64),
                open,
                high,
                low,
                close,
                volume: if breakout {
                    5_000.0
                } else {
                    1_000.0 + idx as f64 * 5.0
                },
            });
        }
        candles
    }
}
