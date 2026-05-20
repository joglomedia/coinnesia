use super::IndicatorPoint;
use crate::Candle;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumePoint {
    pub volume: f64,
    pub average: IndicatorPoint,
    pub ratio: IndicatorPoint,
}

pub fn volume_ratio(candles: &[Candle], period: usize) -> Vec<VolumePoint> {
    assert!(period > 0, "volume period must be greater than zero");
    let mut output = Vec::with_capacity(candles.len());
    let mut rolling = 0.0;

    for (idx, candle) in candles.iter().enumerate() {
        rolling += candle.volume;
        if idx >= period {
            rolling -= candles[idx - period].volume;
        }

        if idx + 1 >= period {
            let average = rolling / period as f64;
            let ratio = if average == 0.0 {
                0.0
            } else {
                candle.volume / average
            };
            output.push(VolumePoint {
                volume: candle.volume,
                average: IndicatorPoint::ready(average),
                ratio: IndicatorPoint::ready(ratio),
            });
        } else {
            output.push(VolumePoint {
                volume: candle.volume,
                average: IndicatorPoint::pending(),
                ratio: IndicatorPoint::pending(),
            });
        }
    }

    output
}
