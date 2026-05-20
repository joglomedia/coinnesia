use super::IndicatorPoint;
use crate::Candle;

pub fn anchored_vwap(candles: &[Candle]) -> Vec<IndicatorPoint> {
    let mut cumulative_pv = 0.0;
    let mut cumulative_volume = 0.0;

    candles
        .iter()
        .map(|candle| {
            let typical = (candle.high + candle.low + candle.close) / 3.0;
            cumulative_pv += typical * candle.volume;
            cumulative_volume += candle.volume;
            if cumulative_volume == 0.0 {
                IndicatorPoint::pending()
            } else {
                IndicatorPoint::ready(cumulative_pv / cumulative_volume)
            }
        })
        .collect()
}
