use std::collections::BTreeMap;

use crate::{
    config::{MtfConfig, StrategyConfig},
    indicators::{
        adx::calculate_dmi,
        ema::calculate_ema,
        smc::{detect_structure, StructureEvent},
        IndicatorPoint,
    },
    AssetClass, Candle, Timeframe,
};

use super::SignalDirection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeframeSet {
    pub primary: Timeframe,
    pub confirmations: Vec<Timeframe>,
}

pub fn parse_timeframe(value: &str) -> Option<Timeframe> {
    match value {
        "1m" | "M1" => Some(Timeframe::M1),
        "5m" | "M5" => Some(Timeframe::M5),
        "15m" | "M15" => Some(Timeframe::M15),
        "1h" | "H1" => Some(Timeframe::H1),
        "4h" | "H4" => Some(Timeframe::H4),
        "1d" | "D1" => Some(Timeframe::D1),
        "1w" | "W1" => Some(Timeframe::W1),
        "1mo" | "MN1" | "Mn1" => Some(Timeframe::Mn1),
        _ => None,
    }
}

pub fn threshold_for_timeframe(timeframe: Timeframe, config: &StrategyConfig) -> f64 {
    match timeframe {
        Timeframe::M1 | Timeframe::M5 | Timeframe::M15 => config.min_confidence_15m,
        Timeframe::H1 => config.min_confidence_1h,
        Timeframe::H4 => config.min_confidence_4h,
        Timeframe::D1 | Timeframe::W1 | Timeframe::Mn1 => config.min_confidence_1d,
    }
}

pub fn timeframe_set(values: &[String]) -> TimeframeSet {
    let mut parsed = values
        .iter()
        .filter_map(|value| parse_timeframe(value))
        .collect::<Vec<_>>();
    if parsed.is_empty() {
        parsed.push(Timeframe::D1);
    }

    let primary = parsed[0];
    TimeframeSet {
        primary,
        confirmations: parsed.into_iter().skip(1).collect(),
    }
}

/// Per-asset-class set of timeframes the scanner must fetch each cycle.
/// Pine references rely on multiple HTF/LTF feeds; for assets where Pine
/// only consumes a subset (Forex, IDX), the set is narrower.
pub fn required_timeframes(asset_class: AssetClass, primary: Timeframe) -> Vec<Timeframe> {
    use Timeframe::*;
    let base: &[Timeframe] = match asset_class {
        AssetClass::Btc | AssetClass::Altcoin => &[M1, M5, M15, H1, H4, D1, W1],
        AssetClass::Gold => &[M15, H1, H4, D1, W1],
        AssetClass::Forex => &[M15, H1, H4, D1],
        AssetClass::StocksIdx | AssetClass::StocksUs => &[H1, H4, D1, W1],
    };
    let mut tfs: Vec<Timeframe> = base.to_vec();
    if !tfs.contains(&primary) {
        tfs.push(primary);
    }
    tfs.sort();
    tfs.dedup();
    tfs
}

/// Container for the candles fetched for a single symbol across all configured
/// timeframes. `primary` is a convenience reference to the symbol's leading TF
/// (used by indicator math); `by_tf` carries the full set keyed by `Timeframe`.
#[derive(Debug, Clone)]
pub struct MtfCandles {
    pub primary: Timeframe,
    pub by_tf: BTreeMap<Timeframe, Vec<Candle>>,
}

impl MtfCandles {
    pub fn new(primary: Timeframe, by_tf: BTreeMap<Timeframe, Vec<Candle>>) -> Self {
        Self { primary, by_tf }
    }

    pub fn primary_candles(&self) -> &[Candle] {
        self.by_tf
            .get(&self.primary)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn candles(&self, tf: Timeframe) -> &[Candle] {
        self.by_tf.get(&tf).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn has(&self, tf: Timeframe) -> bool {
        self.by_tf
            .get(&tf)
            .is_some_and(|series| !series.is_empty())
    }
}

/// Summary computed per timeframe: directional bias, latest BOS/CHOCH event,
/// ADX readiness, and whether close sits above the EMA-200 trend line.
#[derive(Debug, Clone, Copy)]
pub struct TfSummary {
    pub timeframe: Timeframe,
    pub bias: SignalDirection,
    pub structure: StructureEvent,
    pub adx: IndicatorPoint,
    pub ema_trend_above: Option<bool>,
}

impl TfSummary {
    pub fn neutral(timeframe: Timeframe) -> Self {
        Self {
            timeframe,
            bias: SignalDirection::Wait,
            structure: StructureEvent::None,
            adx: IndicatorPoint::pending(),
            ema_trend_above: None,
        }
    }

    pub fn from_candles(
        timeframe: Timeframe,
        candles: &[Candle],
        adx_length: usize,
        adx_smoothing: usize,
        ema_trend_length: usize,
        structure_lookback: usize,
        min_structure_score: f64,
    ) -> Self {
        // The bias check needs at most adx_length+1 candles to be meaningful;
        // ema/structure readiness is allowed to degrade independently so HTFs
        // that lack 200 bars still surface a directional vote.
        if candles.len() < adx_length + 1 {
            return Self::neutral(timeframe);
        }
        let closes: Vec<f64> = candles.iter().map(|candle| candle.close).collect();
        let ema_trend_above = if candles.len() > ema_trend_length {
            let ema_trend = calculate_ema(closes.iter().copied(), ema_trend_length);
            ema_trend
                .last()
                .copied()
                .filter(|p| p.ready)
                .map(|p| candles.last().expect("len > 0").close > p.value)
        } else {
            None
        };
        let dmi_series = calculate_dmi(candles, adx_length, adx_smoothing);
        let dmi_last = dmi_series.last().copied();
        let bias = match dmi_last {
            Some(dmi) if dmi.di_plus.ready && dmi.di_minus.ready => {
                if dmi.di_plus.value > dmi.di_minus.value {
                    SignalDirection::Long
                } else if dmi.di_minus.value > dmi.di_plus.value {
                    SignalDirection::Short
                } else {
                    SignalDirection::Wait
                }
            }
            _ => SignalDirection::Wait,
        };
        let adx = dmi_last
            .map(|dmi| dmi.adx)
            .unwrap_or(IndicatorPoint::pending());
        let structure_event = if candles.len() > structure_lookback {
            detect_structure(candles, structure_lookback, min_structure_score).event
        } else {
            StructureEvent::None
        };

        Self {
            timeframe,
            bias,
            structure: structure_event,
            adx,
            ema_trend_above,
        }
    }
}

/// V61.7 consensus result derived from a fixed LTF triad (M1/M5/M15 when present).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConsensusResult {
    pub long_score: f64,
    pub short_score: f64,
    /// `long_score - short_score`. Positive favours long; negative favours short.
    pub gap: f64,
    /// Number of LTF summaries that contributed votes.
    pub voters: usize,
}

impl ConsensusResult {
    pub const fn neutral() -> Self {
        Self {
            long_score: 0.0,
            short_score: 0.0,
            gap: 0.0,
            voters: 0,
        }
    }

    /// `true` when the consensus opposes `direction` by more than the configured gap.
    pub fn blocks(&self, direction: SignalDirection, config: &MtfConfig) -> bool {
        if self.voters == 0 {
            return false;
        }
        match direction {
            SignalDirection::Long => self.gap <= -config.consensus_score_gap,
            SignalDirection::Short => self.gap >= config.consensus_score_gap,
            SignalDirection::Wait => false,
        }
    }
}

pub fn compute_consensus(summaries: &BTreeMap<Timeframe, TfSummary>) -> ConsensusResult {
    let ltf = [Timeframe::M1, Timeframe::M5, Timeframe::M15];
    let mut long = 0.0;
    let mut short = 0.0;
    let mut voters = 0;
    for tf in ltf {
        if let Some(summary) = summaries.get(&tf) {
            let weight = weight_for_ltf(tf);
            match summary.bias {
                SignalDirection::Long => {
                    long += weight;
                    voters += 1;
                }
                SignalDirection::Short => {
                    short += weight;
                    voters += 1;
                }
                SignalDirection::Wait => {}
            }
        }
    }
    ConsensusResult {
        long_score: long,
        short_score: short,
        gap: long - short,
        voters,
    }
}

fn weight_for_ltf(tf: Timeframe) -> f64 {
    // Pine V61.7 weights the LTF triad equally with a slight lean toward
    // M5 (5-minute reflects micro-flow more reliably than M1 noise).
    match tf {
        Timeframe::M1 => 8.0,
        Timeframe::M5 => 12.0,
        Timeframe::M15 => 10.0,
        _ => 0.0,
    }
}

/// V61.6 microTrend override: scan the last `micro_trend_bars` candles on the
/// lowest available timeframe and return `Long`/`Short` if at least
/// `micro_trend_min_bars` candles move in that direction; otherwise `Wait`.
pub fn micro_trend(candles: &[Candle], config: &MtfConfig) -> SignalDirection {
    if candles.len() < config.micro_trend_bars {
        return SignalDirection::Wait;
    }
    let window = &candles[candles.len() - config.micro_trend_bars..];
    let mut long_count = 0usize;
    let mut short_count = 0usize;
    for candle in window {
        if candle.close > candle.open {
            long_count += 1;
        } else if candle.close < candle.open {
            short_count += 1;
        }
    }
    if long_count >= config.micro_trend_min_bars {
        SignalDirection::Long
    } else if short_count >= config.micro_trend_min_bars {
        SignalDirection::Short
    } else {
        SignalDirection::Wait
    }
}

#[cfg(test)]
mod tests {
    use crate::config::AppConfig;

    use super::*;

    #[test]
    fn maps_timeframes_to_config_thresholds() {
        let config = AppConfig::from_default_toml().expect("default config parses");
        assert_eq!(
            threshold_for_timeframe(Timeframe::M15, &config.strategy),
            config.strategy.min_confidence_15m
        );
        assert_eq!(
            threshold_for_timeframe(Timeframe::H1, &config.strategy),
            config.strategy.min_confidence_1h
        );
        assert_eq!(
            threshold_for_timeframe(Timeframe::D1, &config.strategy),
            config.strategy.min_confidence_1d
        );
    }

    #[test]
    fn required_timeframes_cover_pine_subsets_per_asset() {
        let btc = required_timeframes(AssetClass::Btc, Timeframe::D1);
        assert!(btc.contains(&Timeframe::M1));
        assert!(btc.contains(&Timeframe::H4));
        assert!(btc.contains(&Timeframe::D1));
        let forex = required_timeframes(AssetClass::Forex, Timeframe::H4);
        assert!(forex.contains(&Timeframe::H4));
        assert!(forex.contains(&Timeframe::D1));
        // Forex Pine does not consume M1/M5.
        assert!(!forex.contains(&Timeframe::M1));
        assert!(!forex.contains(&Timeframe::M5));
    }

    #[test]
    fn consensus_blocks_long_when_ltf_triad_opposes() {
        let mut summaries = BTreeMap::new();
        summaries.insert(
            Timeframe::M1,
            TfSummary {
                timeframe: Timeframe::M1,
                bias: SignalDirection::Short,
                structure: StructureEvent::None,
                adx: IndicatorPoint::ready(22.0),
                ema_trend_above: Some(false),
            },
        );
        summaries.insert(
            Timeframe::M5,
            TfSummary {
                timeframe: Timeframe::M5,
                bias: SignalDirection::Short,
                structure: StructureEvent::None,
                adx: IndicatorPoint::ready(24.0),
                ema_trend_above: Some(false),
            },
        );
        summaries.insert(
            Timeframe::M15,
            TfSummary {
                timeframe: Timeframe::M15,
                bias: SignalDirection::Short,
                structure: StructureEvent::None,
                adx: IndicatorPoint::ready(28.0),
                ema_trend_above: Some(false),
            },
        );
        let consensus = compute_consensus(&summaries);
        let mtf = MtfConfig::default();
        assert!(consensus.blocks(SignalDirection::Long, &mtf));
        assert!(!consensus.blocks(SignalDirection::Short, &mtf));
    }

    #[test]
    fn consensus_does_not_block_when_no_voters() {
        let summaries: BTreeMap<Timeframe, TfSummary> = BTreeMap::new();
        let consensus = compute_consensus(&summaries);
        let mtf = MtfConfig::default();
        assert!(!consensus.blocks(SignalDirection::Long, &mtf));
        assert!(!consensus.blocks(SignalDirection::Short, &mtf));
    }

    #[test]
    fn micro_trend_returns_long_when_majority_of_bars_close_up() {
        use chrono::{Duration, Utc};
        let start = Utc::now();
        let candles = (0..5)
            .map(|idx| Candle {
                ts: start + Duration::minutes(idx),
                open: 100.0,
                high: 102.0,
                low: 99.0,
                close: if idx == 0 { 99.5 } else { 101.0 },
                volume: 1_000.0,
            })
            .collect::<Vec<_>>();
        let mtf = MtfConfig::default();
        assert_eq!(micro_trend(&candles, &mtf), SignalDirection::Long);
    }
}
