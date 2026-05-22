use crate::Candle;

use super::{atr::calculate_rma, Indicator, IndicatorPoint};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DmiPoint {
    pub adx: IndicatorPoint,
    pub di_plus: IndicatorPoint,
    pub di_minus: IndicatorPoint,
}

#[derive(Debug, Clone, Copy)]
pub struct Adx {
    pub period: usize,
    pub smoothing: usize,
}

impl Adx {
    pub const fn new(period: usize, smoothing: usize) -> Self {
        Self { period, smoothing }
    }
}

impl Indicator for Adx {
    type Output = Vec<DmiPoint>;

    fn name(&self) -> &'static str {
        "adx"
    }

    fn calculate(&self, candles: &[Candle]) -> Self::Output {
        calculate_dmi(candles, self.period, self.smoothing)
    }
}

pub fn calculate_dmi(candles: &[Candle], period: usize, smoothing: usize) -> Vec<DmiPoint> {
    assert!(period > 0, "DMI period must be greater than zero");
    assert!(smoothing > 0, "ADX smoothing must be greater than zero");

    let mut true_ranges = Vec::with_capacity(candles.len());
    let mut plus_dm = Vec::with_capacity(candles.len());
    let mut minus_dm = Vec::with_capacity(candles.len());

    for (idx, candle) in candles.iter().enumerate() {
        if idx == 0 {
            true_ranges.push(candle.true_range(None));
            plus_dm.push(0.0);
            minus_dm.push(0.0);
            continue;
        }

        let previous = &candles[idx - 1];
        let up_move = candle.high - previous.high;
        let down_move = previous.low - candle.low;

        true_ranges.push(candle.true_range(Some(previous.close)));
        plus_dm.push(if up_move > down_move && up_move > 0.0 {
            up_move
        } else {
            0.0
        });
        minus_dm.push(if down_move > up_move && down_move > 0.0 {
            down_move
        } else {
            0.0
        });
    }

    let tr_rma = calculate_rma(&true_ranges, period);
    let plus_rma = calculate_rma(&plus_dm, period);
    let minus_rma = calculate_rma(&minus_dm, period);

    let mut dx_values = Vec::new();
    let mut dx_indexes = Vec::new();
    let mut output = vec![
        DmiPoint {
            adx: IndicatorPoint::pending(),
            di_plus: IndicatorPoint::pending(),
            di_minus: IndicatorPoint::pending(),
        };
        candles.len()
    ];

    for idx in 0..candles.len() {
        if !(tr_rma[idx].ready && plus_rma[idx].ready && minus_rma[idx].ready) {
            continue;
        }

        let tr = tr_rma[idx].value;
        let di_plus = if tr == 0.0 {
            0.0
        } else {
            100.0 * plus_rma[idx].value / tr
        };
        let di_minus = if tr == 0.0 {
            0.0
        } else {
            100.0 * minus_rma[idx].value / tr
        };
        let sum = di_plus + di_minus;
        let dx = if sum == 0.0 {
            0.0
        } else {
            100.0 * (di_plus - di_minus).abs() / sum
        };

        output[idx].di_plus = IndicatorPoint::ready(di_plus);
        output[idx].di_minus = IndicatorPoint::ready(di_minus);
        dx_values.push(dx);
        dx_indexes.push(idx);
    }

    let adx_values = calculate_rma(&dx_values, smoothing);
    for (adx_idx, point) in adx_values.into_iter().enumerate() {
        if point.ready {
            output[dx_indexes[adx_idx]].adx = point;
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;
    use chrono::{Duration, Utc};

    use super::*;

    fn fixture() -> Vec<Candle> {
        let now = Utc::now();
        [
            (30.0, 28.0, 29.0),
            (32.0, 29.0, 31.0),
            (33.0, 30.0, 32.0),
            (35.0, 31.0, 34.0),
            (34.0, 30.0, 31.0),
            (33.0, 29.0, 30.0),
            (31.0, 27.0, 28.0),
            (30.0, 26.0, 27.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(idx, (high, low, close))| Candle {
            ts: now + Duration::minutes(idx as i64),
            open: close - 0.5,
            high,
            low,
            close,
            volume: 1_000.0,
        })
        .collect()
    }

    #[test]
    fn dmi_uses_wilder_rma_smoothing() {
        let dmi = calculate_dmi(&fixture(), 3, 3);

        assert!(!dmi[3].adx.ready);
        assert_relative_eq!(dmi[2].di_plus.value, 37.5, epsilon = 1e-10);
        assert_relative_eq!(dmi[2].di_minus.value, 0.0, epsilon = 1e-10);
        assert!(dmi[4].adx.ready);
        assert!(dmi[7].di_minus.value > dmi[7].di_plus.value);
    }
}
