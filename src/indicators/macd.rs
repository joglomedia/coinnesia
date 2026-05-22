use crate::Candle;

use super::{ema::calculate_ema, Indicator, IndicatorPoint};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacdPoint {
    pub macd: IndicatorPoint,
    pub signal: IndicatorPoint,
    pub histogram: IndicatorPoint,
}

#[derive(Debug, Clone, Copy)]
pub struct Macd {
    pub fast_period: usize,
    pub slow_period: usize,
    pub signal_period: usize,
}

impl Macd {
    pub const fn new(fast_period: usize, slow_period: usize, signal_period: usize) -> Self {
        Self {
            fast_period,
            slow_period,
            signal_period,
        }
    }
}

impl Indicator for Macd {
    type Output = Vec<MacdPoint>;

    fn name(&self) -> &'static str {
        "macd"
    }

    fn calculate(&self, candles: &[Candle]) -> Self::Output {
        calculate_macd(
            &candles
                .iter()
                .map(|candle| candle.close)
                .collect::<Vec<_>>(),
            self.fast_period,
            self.slow_period,
            self.signal_period,
        )
    }
}

pub fn calculate_macd(
    closes: &[f64],
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
) -> Vec<MacdPoint> {
    assert!(
        fast_period > 0,
        "MACD fast period must be greater than zero"
    );
    assert!(
        slow_period > 0,
        "MACD slow period must be greater than zero"
    );
    assert!(
        signal_period > 0,
        "MACD signal period must be greater than zero"
    );
    assert!(
        fast_period < slow_period,
        "MACD fast period must be less than slow period"
    );

    let fast = calculate_ema(closes.iter().copied(), fast_period);
    let slow = calculate_ema(closes.iter().copied(), slow_period);
    let macd_values: Vec<f64> = fast
        .iter()
        .zip(&slow)
        .map(|(fast, slow)| {
            if fast.ready && slow.ready {
                fast.value - slow.value
            } else {
                f64::NAN
            }
        })
        .collect();
    let signal_values = calculate_ema(
        macd_values.iter().copied().filter(|value| !value.is_nan()),
        signal_period,
    );

    let mut signal_idx = 0;
    macd_values
        .into_iter()
        .map(|macd| {
            if macd.is_nan() {
                return MacdPoint {
                    macd: IndicatorPoint::pending(),
                    signal: IndicatorPoint::pending(),
                    histogram: IndicatorPoint::pending(),
                };
            }
            let signal = signal_values[signal_idx];
            signal_idx += 1;
            let macd = IndicatorPoint::ready(macd);
            let histogram = if signal.ready {
                IndicatorPoint::ready(macd.value - signal.value)
            } else {
                IndicatorPoint::pending()
            };
            MacdPoint {
                macd,
                signal,
                histogram,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;

    use super::*;

    #[test]
    fn macd_seeds_signal_after_ready_macd_values() {
        let closes = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let macd = calculate_macd(&closes, 2, 3, 2);

        assert!(!macd[1].macd.ready);
        assert_relative_eq!(macd[2].macd.value, 0.5, epsilon = 1e-10);
        assert!(!macd[2].signal.ready);
        assert_relative_eq!(macd[3].signal.value, 0.5, epsilon = 1e-10);
        assert_relative_eq!(macd[4].histogram.value, 0.0, epsilon = 1e-10);
    }
}
