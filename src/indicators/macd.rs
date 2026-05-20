use super::{ema::calculate_ema, IndicatorPoint};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacdPoint {
    pub macd: IndicatorPoint,
    pub signal: IndicatorPoint,
    pub histogram: IndicatorPoint,
}

pub fn calculate_macd(
    closes: &[f64],
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
) -> Vec<MacdPoint> {
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
