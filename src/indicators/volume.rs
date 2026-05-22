use super::IndicatorPoint;
use crate::Candle;
use chrono::Timelike;
use chrono_tz::Asia::Jakarta;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumePoint {
    pub volume: f64,
    pub average: IndicatorPoint,
    pub ratio: IndicatorPoint,
    pub z_score: IndicatorPoint,
    pub session_average: IndicatorPoint,
    pub session_ratio: IndicatorPoint,
    pub decaying: bool,
    pub pressure: VolumePressure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumePressure {
    Buy,
    Sell,
    Neutral,
}

pub fn volume_ratio(candles: &[Candle], period: usize) -> Vec<VolumePoint> {
    volume_engine(candles, period)
}

pub fn volume_engine(candles: &[Candle], period: usize) -> Vec<VolumePoint> {
    assert!(period > 0, "volume period must be greater than zero");
    let mut output = Vec::with_capacity(candles.len());
    let mut rolling = 0.0;
    let mut rolling_sq = 0.0;

    for (idx, candle) in candles.iter().enumerate() {
        rolling += candle.volume;
        rolling_sq += candle.volume * candle.volume;
        if idx >= period {
            rolling -= candles[idx - period].volume;
            rolling_sq -= candles[idx - period].volume * candles[idx - period].volume;
        }

        if idx + 1 >= period {
            let average = rolling / period as f64;
            let variance = (rolling_sq / period as f64 - average * average).max(0.0);
            let std_dev = variance.sqrt();
            let ratio = if average == 0.0 {
                0.0
            } else {
                candle.volume / average
            };
            let z_score = if std_dev == 0.0 {
                0.0
            } else {
                (candle.volume - average) / std_dev
            };
            let session_average = session_average(candles, idx, period);
            let session_ratio = if session_average == 0.0 {
                0.0
            } else {
                candle.volume / session_average
            };
            output.push(VolumePoint {
                volume: candle.volume,
                average: IndicatorPoint::ready(average),
                ratio: IndicatorPoint::ready(ratio),
                z_score: IndicatorPoint::ready(z_score),
                session_average: IndicatorPoint::ready(session_average),
                session_ratio: IndicatorPoint::ready(session_ratio),
                decaying: is_decaying(candles, idx),
                pressure: pressure(candle),
            });
        } else {
            output.push(VolumePoint {
                volume: candle.volume,
                average: IndicatorPoint::pending(),
                ratio: IndicatorPoint::pending(),
                z_score: IndicatorPoint::pending(),
                session_average: IndicatorPoint::pending(),
                session_ratio: IndicatorPoint::pending(),
                decaying: false,
                pressure: pressure(candle),
            });
        }
    }

    output
}

fn session_average(candles: &[Candle], idx: usize, period: usize) -> f64 {
    let session = session_bucket(&candles[idx]);
    let mut count = 0;
    let mut sum = 0.0;
    for candle in candles[..=idx].iter().rev() {
        if session_bucket(candle) == session {
            sum += candle.volume;
            count += 1;
            if count == period {
                break;
            }
        }
    }

    if count == 0 {
        0.0
    } else {
        sum / count as f64
    }
}

fn session_bucket(candle: &Candle) -> u8 {
    let local = candle.ts.with_timezone(&Jakarta);
    let minutes = local.hour() * 60 + local.minute();
    if in_range(minutes, 6 * 60, 14 * 60) {
        1
    } else if in_range(minutes, 14 * 60, 22 * 60) {
        2
    } else if in_range(minutes, 19 * 60, 3 * 60) {
        3
    } else if in_range(minutes, 9 * 60, 15 * 60) {
        4
    } else {
        0
    }
}

fn in_range(value: u32, start: u32, end: u32) -> bool {
    if start <= end {
        value >= start && value < end
    } else {
        value >= start || value < end
    }
}

fn pressure(candle: &Candle) -> VolumePressure {
    if candle.close > candle.open {
        VolumePressure::Buy
    } else if candle.close < candle.open {
        VolumePressure::Sell
    } else {
        VolumePressure::Neutral
    }
}

fn is_decaying(candles: &[Candle], idx: usize) -> bool {
    idx >= 2
        && candles[idx].volume < candles[idx - 1].volume
        && candles[idx - 1].volume < candles[idx - 2].volume
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;
    use chrono::{Duration, Utc};

    use super::*;

    #[test]
    fn volume_engine_reports_ratio_zscore_pressure_and_decay() {
        let now = Utc::now();
        let candles = [100.0, 200.0, 300.0, 250.0, 200.0]
            .into_iter()
            .enumerate()
            .map(|(idx, volume)| Candle {
                ts: now + Duration::minutes(idx as i64),
                open: 10.0,
                high: 12.0,
                low: 9.0,
                close: if idx % 2 == 0 { 11.0 } else { 9.5 },
                volume,
            })
            .collect::<Vec<_>>();

        let volume = volume_engine(&candles, 3);
        assert!(!volume[1].ratio.ready);
        assert_relative_eq!(volume[2].average.value, 200.0, epsilon = 1e-10);
        assert_relative_eq!(volume[2].ratio.value, 1.5, epsilon = 1e-10);
        assert_relative_eq!(volume[2].z_score.value, 1.224744871391589, epsilon = 1e-10);
        assert_eq!(volume[3].pressure, VolumePressure::Sell);
        assert!(volume[4].decaying);
    }
}
