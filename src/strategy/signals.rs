use serde::{Deserialize, Serialize};

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
    AssetClass, Candle,
};

use super::{
    confidence::ConfidenceScore,
    entry_plan::{EntryPlan, EntryPlanCalculator},
    mtf::{threshold_for_timeframe, timeframe_set},
    session::{classify_wib, session_allows_asset},
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
        if candles.len() < self.config.indicators.atr_length + 1 {
            return SignalResult::wait(symbol, "not_enough_candles");
        }

        let Some(symbol_config) = self
            .config
            .symbols
            .iter()
            .find(|item| item.symbol == symbol)
        else {
            return SignalResult::wait(symbol, "symbol_not_configured");
        };
        let timeframe = timeframe_set(&symbol_config.timeframes).primary;
        let snapshot = IndicatorSnapshot::new(candles, self.config);
        let regime = snapshot.regime;
        if regime == MarketRegime::Shock {
            return SignalResult::freeze(symbol, "shock_regime");
        }
        if !regime.allows_signals() {
            return SignalResult::wait(symbol, format!("regime_block:{regime:?}"));
        }

        let session = classify_wib(snapshot.latest.ts);
        if !session_allows_asset(session, symbol_config.asset_class) {
            return SignalResult::wait(symbol, format!("session_block:{session:?}"));
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
            return SignalResult::wait(symbol, "trap_guard_blocked");
        }

        let state = if long.passes && confidence.long > confidence.short {
            SignalState::Long
        } else if short.passes && confidence.short > confidence.long {
            SignalState::Short
        } else {
            SignalState::Wait
        };

        if state == SignalState::Wait
            || !confidence.passes(threshold, self.config.strategy.min_directional_gap)
        {
            return SignalResult {
                symbol: symbol.to_owned(),
                state: SignalState::Wait,
                confidence,
                reason: format!(
                    "layers_not_met threshold={threshold:.1} long={} short={}",
                    long.reason, short.reason
                ),
                entry_plan: None,
            };
        }

        SignalResult {
            symbol: symbol.to_owned(),
            state,
            confidence,
            reason: format!("six_layer_pass timeframe={timeframe:?} session={session:?}"),
            entry_plan: None,
        }
        .with_entry_plan(self.config, snapshot.latest.close, snapshot.atr.value)
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
}

impl<'a> IndicatorSnapshot<'a> {
    fn new(candles: &'a [Candle], config: &AppConfig) -> Self {
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
    let entry = entry_layer(direction, snapshot);
    let anti_trap = anti_trap_layer(direction, snapshot, config);
    let regime_session = snapshot.regime.allows_signals();

    add_layer(
        &mut score,
        profile,
        &["structure", "ema", "ema_htf", "htf_bias"],
        trend,
        &mut missing,
        "trend",
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
            snapshot.rsi.ready
                && ((50.0..=72.0).contains(&snapshot.rsi.value) || snapshot.rsi.value > 30.0)
        }
        SignalDirection::Short => {
            snapshot.rsi.ready
                && ((28.0..=50.0).contains(&snapshot.rsi.value) || snapshot.rsi.value < 70.0)
        }
        SignalDirection::Wait => false,
    };
    let macd = match direction {
        SignalDirection::Long => {
            snapshot.macd.macd.ready
                && snapshot.macd.signal.ready
                && snapshot.macd.macd.value >= snapshot.macd.signal.value
        }
        SignalDirection::Short => {
            snapshot.macd.macd.ready
                && snapshot.macd.signal.ready
                && snapshot.macd.macd.value <= snapshot.macd.signal.value
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

fn entry_layer(direction: SignalDirection, snapshot: &IndicatorSnapshot<'_>) -> bool {
    let atr_tolerance = snapshot.atr.value.max(0.0) * 0.08;
    let sweep = detect_liquidity_sweep(
        snapshot.candles,
        20.min(snapshot.candles.len() - 1),
        atr_tolerance,
    );
    let order_blocks = detect_order_blocks(snapshot.candles, 20, 0.55);
    let zones = detect_zones(snapshot.candles, 30, snapshot.atr.value.max(0.0) * 0.2, 6);

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

    fn test_config() -> AppConfig {
        let mut config = AppConfig::from_default_toml().expect("default config parses");
        config.strategy.min_confidence_15m = 45.0;
        config.strategy.min_confidence_1h = 45.0;
        config.strategy.min_confidence_4h = 45.0;
        config.strategy.min_confidence_1d = 45.0;
        config.strategy.min_directional_gap = 5.0;
        config.strategy.min_structure_score = 5.0;
        config
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
