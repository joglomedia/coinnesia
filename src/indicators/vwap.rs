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
            // Pine three-session boundaries (BTC V61.9 `f_session_from_minutes`).
            // Idx and RolloverAvoid overlays do NOT apply here — session VWAP must
            // reset at the same cutoffs Pine uses (07:00, 15:00, 20:30 WIB).
            let local = candle.ts.with_timezone(&Jakarta);
            let minutes = local.hour() * 60 + local.minute();
            let (session_id, anchor_date) = if minutes >= 7 * 60 && minutes < 15 * 60 {
                (1_i64, local.date_naive())
            } else if minutes >= 15 * 60 && minutes < 20 * 60 + 30 {
                (2_i64, local.date_naive())
            } else if minutes >= 20 * 60 + 30 {
                // USA evening segment 20:30-23:59 — anchored to today.
                (3_i64, local.date_naive())
            } else {
                // USA early segment 00:00-06:59 — anchored to PREVIOUS day so the
                // single USA session that spans 20:30 → 06:59 keeps one cumulator.
                (3_i64, local.date_naive() - chrono::Duration::days(1))
            };
            let date_key = i64::from(anchor_date.year()) * 10_000
                + i64::from(anchor_date.month()) * 100
                + i64::from(anchor_date.day());
            date_key * 10 + session_id
        }
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
        // 14:30 WIB (07:30 UTC) — Asia bucket.
        // 15:30 WIB (08:30 UTC) — Europe bucket (Pine cutoff at 15:00).
        let candles = vec![
            candle(
                Utc.with_ymd_and_hms(2026, 5, 20, 7, 30, 0).unwrap(),
                100.0,
                10.0,
            ),
            candle(
                Utc.with_ymd_and_hms(2026, 5, 20, 8, 30, 0).unwrap(),
                120.0,
                10.0,
            ),
        ];
        let vwap = session_vwap(&candles);

        assert_relative_eq!(vwap[0].value, 100.0, epsilon = 1e-10);
        assert_relative_eq!(vwap[1].value, 120.0, epsilon = 1e-10);
    }

    #[test]
    fn session_vwap_resets_at_pine_usa_boundary() {
        // 20:00 WIB (13:00 UTC) — Europe.
        // 21:00 WIB (14:00 UTC) — Usa (Pine cutoff at 20:30).
        let candles = vec![
            candle(
                Utc.with_ymd_and_hms(2026, 5, 20, 13, 0, 0).unwrap(),
                100.0,
                10.0,
            ),
            candle(
                Utc.with_ymd_and_hms(2026, 5, 20, 14, 0, 0).unwrap(),
                140.0,
                10.0,
            ),
        ];
        let vwap = session_vwap(&candles);

        assert_relative_eq!(vwap[0].value, 100.0, epsilon = 1e-10);
        // VWAP must reset at 20:30 WIB — pre-fix this incorrectly stayed cumulative.
        assert_relative_eq!(vwap[1].value, 140.0, epsilon = 1e-10);
    }

    #[test]
    fn session_vwap_does_not_reset_across_midnight_inside_usa_session() {
        // 22:00 WIB May 20 (15:00 UTC) — Usa (evening segment, anchor = May 20).
        // 01:00 WIB May 21 (18:00 UTC May 20) — Usa (early segment, anchor = May 20).
        // Both bars must share the cumulator because Pine USA runs 20:30 → 02:59.
        let candles = vec![
            candle(
                Utc.with_ymd_and_hms(2026, 5, 20, 15, 0, 0).unwrap(),
                100.0,
                10.0,
            ),
            candle(
                Utc.with_ymd_and_hms(2026, 5, 20, 18, 0, 0).unwrap(),
                200.0,
                10.0,
            ),
        ];
        let vwap = session_vwap(&candles);

        assert_relative_eq!(vwap[0].value, 100.0, epsilon = 1e-10);
        // Cumulative VWAP across both bars = (100*10 + 200*10) / 20 = 150.
        assert_relative_eq!(vwap[1].value, 150.0, epsilon = 1e-10);
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
