use crate::Candle;

use super::IndicatorPoint;

/// Pine V5 IDX:
///   mfMul = ((close - low) - (high - close)) / (high - low)
///   mfVol = mfMul * volume
///   cmf   = sum(mfVol, len) / max(sum(volume, len), 1)
///
/// `len` defaults to 20. We emit one [`IndicatorPoint`] per candle; values are
/// pending until at least `len` bars have been observed.
pub fn calculate_cmf(candles: &[Candle], length: usize) -> Vec<IndicatorPoint> {
    assert!(length > 0, "CMF length must be greater than zero");
    let mut output = Vec::with_capacity(candles.len());
    let mut mfv_sum = 0.0_f64;
    let mut vol_sum = 0.0_f64;
    let mut buf_mfv = Vec::with_capacity(length);
    let mut buf_vol = Vec::with_capacity(length);

    for (idx, candle) in candles.iter().enumerate() {
        let range = candle.high - candle.low;
        let mf_mul = if range > 0.0 {
            ((candle.close - candle.low) - (candle.high - candle.close)) / range
        } else {
            0.0
        };
        let mfv = mf_mul * candle.volume;

        buf_mfv.push(mfv);
        buf_vol.push(candle.volume);
        mfv_sum += mfv;
        vol_sum += candle.volume;

        if buf_mfv.len() > length {
            mfv_sum -= buf_mfv.remove(0);
            vol_sum -= buf_vol.remove(0);
        }

        if idx + 1 >= length {
            let denom = vol_sum.max(1.0);
            output.push(IndicatorPoint::ready(mfv_sum / denom));
        } else {
            output.push(IndicatorPoint::pending());
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    fn make_candle(idx: usize, open: f64, high: f64, low: f64, close: f64, volume: f64) -> Candle {
        Candle {
            ts: Utc::now() + Duration::minutes(idx as i64),
            open,
            high,
            low,
            close,
            volume,
        }
    }

    #[test]
    fn cmf_pending_until_length_reached() {
        let candles: Vec<Candle> = (0..5)
            .map(|i| make_candle(i, 100.0, 101.0, 99.0, 100.5, 1_000.0))
            .collect();
        let cmf = calculate_cmf(&candles, 5);
        assert!(!cmf[0].ready);
        assert!(!cmf[3].ready);
        assert!(cmf[4].ready);
    }

    #[test]
    fn cmf_positive_when_close_in_upper_range() {
        // close at high → mfMul = +1, cmf approaches +1
        let candles: Vec<Candle> = (0..20)
            .map(|i| make_candle(i, 100.0, 102.0, 98.0, 102.0, 1_000.0))
            .collect();
        let cmf = calculate_cmf(&candles, 20);
        let last = cmf.last().unwrap();
        assert!(last.ready);
        assert!(last.value > 0.99, "expected near +1, got {}", last.value);
    }

    #[test]
    fn cmf_negative_when_close_in_lower_range() {
        // close at low → mfMul = -1
        let candles: Vec<Candle> = (0..20)
            .map(|i| make_candle(i, 100.0, 102.0, 98.0, 98.0, 1_000.0))
            .collect();
        let cmf = calculate_cmf(&candles, 20);
        let last = cmf.last().unwrap();
        assert!(last.ready);
        assert!(last.value < -0.99, "expected near -1, got {}", last.value);
    }

    #[test]
    fn cmf_zero_volume_or_zero_range_safe() {
        // Range zero → mfMul defaults to 0; volume zero → denominator clamped to 1.
        let candles: Vec<Candle> = (0..20)
            .map(|i| make_candle(i, 100.0, 100.0, 100.0, 100.0, 0.0))
            .collect();
        let cmf = calculate_cmf(&candles, 20);
        assert_eq!(cmf.last().unwrap().value, 0.0);
    }
}
