use crate::Candle;

use super::{Indicator, IndicatorPoint};

#[derive(Debug, Clone, Copy)]
pub struct Ema {
    pub period: usize,
}

impl Ema {
    pub const fn new(period: usize) -> Self {
        Self { period }
    }
}

impl Indicator for Ema {
    type Output = Vec<IndicatorPoint>;

    fn name(&self) -> &'static str {
        "ema"
    }

    fn calculate(&self, candles: &[Candle]) -> Self::Output {
        calculate_ema(candles.iter().map(|c| c.close), self.period)
    }
}

pub fn calculate_ema(values: impl IntoIterator<Item = f64>, period: usize) -> Vec<IndicatorPoint> {
    assert!(period > 0, "EMA period must be greater than zero");
    let multiplier = 2.0 / (period as f64 + 1.0);
    let mut output = Vec::new();
    let mut seed = Vec::with_capacity(period);
    let mut ema = None;

    for value in values {
        match ema {
            Some(prev) => {
                let next = (value - prev) * multiplier + prev;
                ema = Some(next);
                output.push(IndicatorPoint::ready(next));
            }
            None => {
                seed.push(value);
                if seed.len() == period {
                    let sma = seed.iter().sum::<f64>() / period as f64;
                    ema = Some(sma);
                    output.push(IndicatorPoint::ready(sma));
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
    fn ema_seeds_with_sma_then_smooths() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0];
        let ema = calculate_ema(values, 3);
        assert!(!ema[0].ready);
        assert_relative_eq!(ema[2].value, 2.0, epsilon = 1e-10);
        assert_relative_eq!(ema[3].value, 3.0, epsilon = 1e-10);
        assert_relative_eq!(ema[4].value, 4.0, epsilon = 1e-10);
    }
}
