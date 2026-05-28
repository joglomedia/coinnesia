use crate::Candle;

use super::IndicatorPoint;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObvPoint {
    /// Cumulative on-balance volume up to this bar.
    pub obv: IndicatorPoint,
    /// `obv - obv[slope_length]` — positive ⇒ accumulation slope.
    pub slope: IndicatorPoint,
}

/// Pine V5 IDX:
///   obv = ta.cum(math.sign(ta.change(close)) * volume)
///   obvSlope = obv - obv[slopeLen]
///   obvOK = obvSlope > 0
///
/// `slope_length` is Pine's `obvLen` (default 10).
pub fn calculate_obv(candles: &[Candle], slope_length: usize) -> Vec<ObvPoint> {
    assert!(slope_length > 0, "OBV slope length must be greater than zero");
    let mut output = Vec::with_capacity(candles.len());
    let mut obv = 0.0_f64;
    let mut history = Vec::with_capacity(candles.len());

    for (idx, candle) in candles.iter().enumerate() {
        let prev_close = if idx == 0 {
            candle.close
        } else {
            candles[idx - 1].close
        };
        let direction = if candle.close > prev_close {
            1.0
        } else if candle.close < prev_close {
            -1.0
        } else {
            0.0
        };
        obv += direction * candle.volume;
        history.push(obv);

        let slope_idx = idx as isize - slope_length as isize;
        let slope_point = if slope_idx >= 0 {
            IndicatorPoint::ready(obv - history[slope_idx as usize])
        } else {
            IndicatorPoint::pending()
        };

        output.push(ObvPoint {
            obv: IndicatorPoint::ready(obv),
            slope: slope_point,
        });
    }

    output
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    fn make_candles(closes: &[f64]) -> Vec<Candle> {
        let now = Utc::now();
        closes
            .iter()
            .enumerate()
            .map(|(idx, &close)| Candle {
                ts: now + Duration::minutes(idx as i64),
                open: close,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 1_000.0,
            })
            .collect()
    }

    #[test]
    fn obv_accumulates_on_up_close_and_distributes_on_down_close() {
        let candles = make_candles(&[10.0, 11.0, 12.0, 11.0, 13.0]);
        let obv = calculate_obv(&candles, 2);
        // bar0: direction = 0 (no prior) → 0
        // bar1: up → +1000
        // bar2: up → +2000
        // bar3: down → +1000
        // bar4: up → +2000
        assert_eq!(obv[0].obv.value, 0.0);
        assert_eq!(obv[1].obv.value, 1_000.0);
        assert_eq!(obv[2].obv.value, 2_000.0);
        assert_eq!(obv[3].obv.value, 1_000.0);
        assert_eq!(obv[4].obv.value, 2_000.0);
    }

    #[test]
    fn slope_pending_until_lookback_satisfied_then_signals_direction() {
        let candles = make_candles(&[10.0, 11.0, 12.0, 13.0, 14.0]);
        let obv = calculate_obv(&candles, 3);
        assert!(!obv[2].slope.ready);
        assert!(obv[3].slope.ready);
        // obv[3]=3000, obv[0]=0 → slope=+3000
        assert_eq!(obv[3].slope.value, 3_000.0);
    }
}
