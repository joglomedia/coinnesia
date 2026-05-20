use crate::Candle;

use super::{Indicator, IndicatorPoint};

#[derive(Debug, Clone, Copy)]
pub struct Atr {
    pub period: usize,
}

impl Atr {
    pub const fn new(period: usize) -> Self {
        Self { period }
    }
}

impl Indicator for Atr {
    type Output = Vec<IndicatorPoint>;

    fn name(&self) -> &'static str {
        "atr"
    }

    fn calculate(&self, candles: &[Candle]) -> Self::Output {
        calculate_atr(candles, self.period)
    }
}

pub fn calculate_atr(candles: &[Candle], period: usize) -> Vec<IndicatorPoint> {
    assert!(period > 0, "ATR period must be greater than zero");
    let true_ranges: Vec<f64> = candles
        .iter()
        .enumerate()
        .map(|(idx, candle)| {
            let prev_close = idx.checked_sub(1).map(|prev| candles[prev].close);
            candle.true_range(prev_close)
        })
        .collect();

    calculate_rma(&true_ranges, period)
}

pub fn calculate_rma(values: &[f64], period: usize) -> Vec<IndicatorPoint> {
    assert!(period > 0, "RMA period must be greater than zero");
    let mut output = Vec::with_capacity(values.len());
    let mut seed_sum = 0.0;
    let mut rma = None;

    for (idx, value) in values.iter().copied().enumerate() {
        match rma {
            Some(prev) => {
                let next = (prev * (period as f64 - 1.0) + value) / period as f64;
                rma = Some(next);
                output.push(IndicatorPoint::ready(next));
            }
            None => {
                seed_sum += value;
                if idx + 1 == period {
                    let first = seed_sum / period as f64;
                    rma = Some(first);
                    output.push(IndicatorPoint::ready(first));
                } else {
                    output.push(IndicatorPoint::pending());
                }
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;

    use super::*;

    #[test]
    fn rma_matches_wilder_smoothing() {
        let values = [1.0, 2.0, 3.0, 4.0];
        let rma = calculate_rma(&values, 3);
        assert!(!rma[1].ready);
        assert_relative_eq!(rma[2].value, 2.0, epsilon = 1e-10);
        assert_relative_eq!(rma[3].value, (2.0 * 2.0 + 4.0) / 3.0, epsilon = 1e-10);
    }
}
