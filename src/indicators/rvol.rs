use crate::Candle;

use super::IndicatorPoint;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RvolPoint {
    /// `volume / SMA(volume, length)` — Pine `rvol`.
    pub rvol: IndicatorPoint,
    /// `close * volume` — Pine `valueTraded`.
    pub value_traded: f64,
    /// `SMA(close * volume, length)` — Pine `avgValue`.
    pub avg_value: IndicatorPoint,
    /// `value_traded / avg_value` — Pine implicit value gate ratio.
    pub value_ratio: IndicatorPoint,
}

/// Pine V5 IDX:
///   rvol        = volSma > 0 ? volume / volSma : 0
///   valueTraded = close * volume
///   avgValue    = ta.sma(valueTraded, volLen)
///   rvolOK      = rvol >= rvolMin (default 1.20)
///
/// `length` is Pine's `volLen` (default 20). `rvolOK` is exposed via
/// [`is_rvol_ok`] so callers can supply the configurable threshold.
pub fn calculate_rvol(candles: &[Candle], length: usize) -> Vec<RvolPoint> {
    assert!(length > 0, "RVOL length must be greater than zero");
    let mut output = Vec::with_capacity(candles.len());
    let mut vol_sum = 0.0_f64;
    let mut val_sum = 0.0_f64;
    let mut vol_buf = Vec::with_capacity(length);
    let mut val_buf = Vec::with_capacity(length);

    for (idx, candle) in candles.iter().enumerate() {
        let value_traded = candle.close * candle.volume;
        vol_sum += candle.volume;
        val_sum += value_traded;
        vol_buf.push(candle.volume);
        val_buf.push(value_traded);
        if vol_buf.len() > length {
            vol_sum -= vol_buf.remove(0);
            val_sum -= val_buf.remove(0);
        }

        if idx + 1 >= length {
            let vol_sma = vol_sum / length as f64;
            let val_sma = val_sum / length as f64;
            let rvol = if vol_sma > 0.0 {
                candle.volume / vol_sma
            } else {
                0.0
            };
            let val_ratio = if val_sma > 0.0 {
                value_traded / val_sma
            } else {
                0.0
            };
            output.push(RvolPoint {
                rvol: IndicatorPoint::ready(rvol),
                value_traded,
                avg_value: IndicatorPoint::ready(val_sma),
                value_ratio: IndicatorPoint::ready(val_ratio),
            });
        } else {
            output.push(RvolPoint {
                rvol: IndicatorPoint::pending(),
                value_traded,
                avg_value: IndicatorPoint::pending(),
                value_ratio: IndicatorPoint::pending(),
            });
        }
    }

    output
}

/// Pine `rvolOK = rvol >= rvolMin`. Treats pending values as `false`.
pub fn is_rvol_ok(point: &RvolPoint, rvol_min: f64) -> bool {
    point.rvol.ready && point.rvol.value >= rvol_min
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;
    use chrono::{Duration, Utc};

    use super::*;

    fn make_candle(idx: usize, close: f64, volume: f64) -> Candle {
        Candle {
            ts: Utc::now() + Duration::minutes(idx as i64),
            open: close,
            high: close + 0.5,
            low: close - 0.5,
            close,
            volume,
        }
    }

    #[test]
    fn rvol_pending_until_length() {
        let candles: Vec<Candle> = (0..20)
            .map(|i| make_candle(i, 100.0, 1_000.0))
            .collect();
        let rvol = calculate_rvol(&candles, 20);
        assert!(!rvol[18].rvol.ready);
        assert!(rvol[19].rvol.ready);
    }

    #[test]
    fn rvol_one_when_volume_matches_sma() {
        let candles: Vec<Candle> = (0..20)
            .map(|i| make_candle(i, 100.0, 1_000.0))
            .collect();
        let rvol = calculate_rvol(&candles, 20);
        assert_relative_eq!(rvol[19].rvol.value, 1.0, epsilon = 1e-10);
        assert!(!is_rvol_ok(&rvol[19], 1.20));
    }

    #[test]
    fn rvol_above_threshold_when_volume_spikes() {
        let mut candles: Vec<Candle> = (0..19)
            .map(|i| make_candle(i, 100.0, 1_000.0))
            .collect();
        candles.push(make_candle(19, 100.0, 1_500.0));
        let rvol = calculate_rvol(&candles, 20);
        // SMA over 1000*19 + 1500 = 20500 / 20 = 1025; rvol = 1500/1025 ≈ 1.4634
        assert_relative_eq!(rvol[19].rvol.value, 1500.0 / 1025.0, epsilon = 1e-10);
        assert!(is_rvol_ok(&rvol[19], 1.20));
    }

    #[test]
    fn value_traded_tracks_close_times_volume() {
        let candles = vec![make_candle(0, 200.0, 500.0)];
        let rvol = calculate_rvol(&candles, 1);
        assert_eq!(rvol[0].value_traded, 100_000.0);
    }
}
