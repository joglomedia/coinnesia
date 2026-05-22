use chrono::{Datelike, Timelike};
use chrono_tz::Asia::Jakarta;

use super::IndicatorPoint;
use crate::Candle;

pub fn anchored_vwap(candles: &[Candle]) -> Vec<IndicatorPoint> {
    anchored_vwap_with_anchor(candles, VwapAnchor::Continuous)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VwapAnchor {
    Continuous,
    DailyWib,
    SessionWib,
}

pub fn session_vwap(candles: &[Candle]) -> Vec<IndicatorPoint> {
    anchored_vwap_with_anchor(candles, VwapAnchor::SessionWib)
}

pub fn daily_vwap_wib(candles: &[Candle]) -> Vec<IndicatorPoint> {
    anchored_vwap_with_anchor(candles, VwapAnchor::DailyWib)
}

pub fn anchored_vwap_with_anchor(candles: &[Candle], anchor: VwapAnchor) -> Vec<IndicatorPoint> {
    let mut cumulative_pv = 0.0;
    let mut cumulative_volume = 0.0;
    let mut current_anchor = None;

    candles
        .iter()
        .map(|candle| {
            let anchor_key = anchor_key(candle, anchor);
            if current_anchor != Some(anchor_key) {
                cumulative_pv = 0.0;
                cumulative_volume = 0.0;
                current_anchor = Some(anchor_key);
            }

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

fn anchor_key(candle: &Candle, anchor: VwapAnchor) -> i64 {
    match anchor {
        VwapAnchor::Continuous => 0,
        VwapAnchor::DailyWib => {
            let local = candle.ts.with_timezone(&Jakarta);
            i64::from(local.year()) * 10_000
                + i64::from(local.month()) * 100
                + i64::from(local.day())
        }
        VwapAnchor::SessionWib => {
            let local = candle.ts.with_timezone(&Jakarta);
            let minutes = local.hour() * 60 + local.minute();
            let session = if in_range(minutes, 6 * 60, 14 * 60) {
                1
            } else if in_range(minutes, 14 * 60, 22 * 60) {
                2
            } else if in_range(minutes, 19 * 60, 3 * 60) {
                3
            } else if in_range(minutes, 9 * 60, 15 * 60) {
                4
            } else {
                0
            };
            (i64::from(local.year()) * 10_000
                + i64::from(local.month()) * 100
                + i64::from(local.day()))
                * 10
                + session
        }
    }
}

fn in_range(value: u32, start: u32, end: u32) -> bool {
    if start <= end {
        value >= start && value < end
    } else {
        value >= start || value < end
    }
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn daily_vwap_resets_on_wib_day_boundary() {
        let candles = vec![
            candle(
                Utc.with_ymd_and_hms(2026, 5, 20, 15, 0, 0).unwrap(),
                100.0,
                10.0,
            ),
            candle(
                Utc.with_ymd_and_hms(2026, 5, 20, 17, 0, 0).unwrap(),
                110.0,
                10.0,
            ),
        ];
        let vwap = daily_vwap_wib(&candles);

        assert_relative_eq!(vwap[0].value, 100.0, epsilon = 1e-10);
        assert_relative_eq!(vwap[1].value, 110.0, epsilon = 1e-10);
    }

    #[test]
    fn session_vwap_resets_between_asia_and_europe() {
        let candles = vec![
            candle(
                Utc.with_ymd_and_hms(2026, 5, 20, 6, 0, 0).unwrap(),
                100.0,
                10.0,
            ),
            candle(
                Utc.with_ymd_and_hms(2026, 5, 20, 7, 0, 0).unwrap(),
                120.0,
                10.0,
            ),
        ];
        let vwap = session_vwap(&candles);

        assert_relative_eq!(vwap[0].value, 100.0, epsilon = 1e-10);
        assert_relative_eq!(vwap[1].value, 120.0, epsilon = 1e-10);
    }

    fn candle(ts: chrono::DateTime<Utc>, price: f64, volume: f64) -> Candle {
        Candle {
            ts,
            open: price,
            high: price,
            low: price,
            close: price,
            volume,
        }
    }
}
