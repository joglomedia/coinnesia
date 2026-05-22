use crate::Candle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureEvent {
    BullishBos,
    BearishBos,
    BullishChoch,
    BearishChoch,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StructureState {
    pub event: StructureEvent,
    pub score: f64,
}

pub fn detect_structure(
    candles: &[Candle],
    lookback: usize,
    min_structure_score: f64,
) -> StructureState {
    if candles.len() < lookback.max(3) + 1 {
        return StructureState {
            event: StructureEvent::None,
            score: 0.0,
        };
    }

    let latest = candles.last().expect("len checked");
    let start = candles.len().saturating_sub(lookback + 1);
    let prior = &candles[start..candles.len() - 1];
    let swing_high = prior
        .iter()
        .map(|candle| candle.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let swing_low = prior
        .iter()
        .map(|candle| candle.low)
        .fold(f64::INFINITY, f64::min);
    let range = (swing_high - swing_low).max(f64::EPSILON);
    let previous_bias = prior
        .last()
        .zip(prior.first())
        .map(|(last, first)| last.close - first.close)
        .unwrap_or(0.0);

    let (event, break_distance) = if latest.close > swing_high {
        let event = if previous_bias < 0.0 {
            StructureEvent::BullishChoch
        } else {
            StructureEvent::BullishBos
        };
        (event, latest.close - swing_high)
    } else if latest.close < swing_low {
        let event = if previous_bias > 0.0 {
            StructureEvent::BearishChoch
        } else {
            StructureEvent::BearishBos
        };
        (event, swing_low - latest.close)
    } else {
        (StructureEvent::None, 0.0)
    };

    let score = ((break_distance / range) * 100.0).clamp(0.0, 100.0);
    if score < min_structure_score {
        StructureState {
            event: StructureEvent::None,
            score,
        }
    } else {
        StructureState { event, score }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    #[test]
    fn detects_bullish_choch_after_down_bias_breaks_swing_high() {
        let now = Utc::now();
        let closes = [100.0, 98.0, 96.0, 94.0, 93.0, 112.0];
        let candles = closes
            .into_iter()
            .enumerate()
            .map(|(idx, close)| Candle {
                ts: now + Duration::minutes(idx as i64),
                open: close - 1.0,
                high: close + 2.0,
                low: close - 2.0,
                close,
                volume: 1_000.0,
            })
            .collect::<Vec<_>>();

        let state = detect_structure(&candles, 5, 10.0);
        assert_eq!(state.event, StructureEvent::BullishChoch);
        assert!(state.score >= 10.0);
    }
}
