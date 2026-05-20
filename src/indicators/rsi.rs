use crate::Candle;

use super::{Indicator, IndicatorPoint};

#[derive(Debug, Clone, Copy)]
pub struct Rsi {
    pub period: usize,
}

impl Rsi {
    pub const fn new(period: usize) -> Self {
        Self { period }
    }
}

impl Indicator for Rsi {
    type Output = Vec<IndicatorPoint>;

    fn name(&self) -> &'static str {
        "rsi"
    }

    fn calculate(&self, candles: &[Candle]) -> Self::Output {
        calculate_rsi(
            candles
                .iter()
                .map(|c| c.close)
                .collect::<Vec<_>>()
                .as_slice(),
            self.period,
        )
    }
}

pub fn calculate_rsi(closes: &[f64], period: usize) -> Vec<IndicatorPoint> {
    assert!(period > 0, "RSI period must be greater than zero");
    if closes.is_empty() {
        return Vec::new();
    }

    let mut output = vec![IndicatorPoint::pending(); closes.len()];
    if closes.len() <= period {
        return output;
    }

    let mut gain_sum = 0.0;
    let mut loss_sum = 0.0;
    for idx in 1..=period {
        let change = closes[idx] - closes[idx - 1];
        gain_sum += change.max(0.0);
        loss_sum += (-change).max(0.0);
    }

    let mut avg_gain = gain_sum / period as f64;
    let mut avg_loss = loss_sum / period as f64;
    output[period] = IndicatorPoint::ready(rsi_from_averages(avg_gain, avg_loss));

    for idx in (period + 1)..closes.len() {
        let change = closes[idx] - closes[idx - 1];
        avg_gain = (avg_gain * (period as f64 - 1.0) + change.max(0.0)) / period as f64;
        avg_loss = (avg_loss * (period as f64 - 1.0) + (-change).max(0.0)) / period as f64;
        output[idx] = IndicatorPoint::ready(rsi_from_averages(avg_gain, avg_loss));
    }

    output
}

fn rsi_from_averages(avg_gain: f64, avg_loss: f64) -> f64 {
    match (avg_gain, avg_loss) {
        (0.0, 0.0) => 50.0,
        (_, 0.0) => 100.0,
        (0.0, _) => 0.0,
        (avg_gain, avg_loss) => {
            let rs = avg_gain / avg_loss;
            100.0 - (100.0 / (1.0 + rs))
        }
    }
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;

    use super::*;

    #[test]
    fn rsi_uses_rma_not_sma() {
        let closes = [
            44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03,
            45.61, 46.28, 46.28, 46.00, 46.03,
        ];
        let rsi = calculate_rsi(&closes, 14);
        assert_relative_eq!(rsi[14].value, 70.46413502109705, epsilon = 1e-10);
        assert_relative_eq!(rsi[15].value, 66.24961855355505, epsilon = 1e-10);
        assert_relative_eq!(rsi[16].value, 66.48094183471265, epsilon = 1e-10);
    }
}
