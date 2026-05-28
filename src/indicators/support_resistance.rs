use crate::Candle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneKind {
    Support,
    Resistance,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriceZone {
    pub kind: ZoneKind,
    pub low: f64,
    pub high: f64,
    pub strength: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NearLocation {
    NearResistance,
    NearSupport,
    Clear,
}

/// Pine V5 IDX `srResistanceStrength` + V61.7 target-block-SR. Combines:
/// - intraday pivot zones from [`detect_zones`]
/// - daily H/L levels (price extremes within `daily_lookback`)
/// - weekly H/L levels (price extremes within `weekly_lookback`)
///
/// Strength of a daily/weekly level is fixed at the seed weight; pivot zones
/// keep their own pivot count.
#[derive(Debug, Clone)]
pub struct SrSnapshot {
    pub zones: Vec<PriceZone>,
    pub daily_high: f64,
    pub daily_low: f64,
    pub weekly_high: f64,
    pub weekly_low: f64,
}

/// Pine V5 panel: maps the V61.7 `priceNearResistance` flag plus its support
/// mirror from a snapshot. `near_atr` is the configurable `srNearATR` band.
pub fn near_status(snapshot: &SrSnapshot, price: f64, atr: f64, near_atr: f64) -> NearLocation {
    let band = atr * near_atr.max(0.0);
    let near_resistance = snapshot
        .zones
        .iter()
        .filter(|z| z.kind == ZoneKind::Resistance)
        .any(|z| price >= z.low - band && price <= z.high + band)
        || (price - snapshot.daily_high).abs() <= band
        || (price - snapshot.weekly_high).abs() <= band;
    let near_support = snapshot
        .zones
        .iter()
        .filter(|z| z.kind == ZoneKind::Support)
        .any(|z| price >= z.low - band && price <= z.high + band)
        || (price - snapshot.daily_low).abs() <= band
        || (price - snapshot.weekly_low).abs() <= band;

    match (near_resistance, near_support) {
        (true, false) => NearLocation::NearResistance,
        (false, true) => NearLocation::NearSupport,
        (true, true) => {
            // Tie-break by which band centre is closer to price.
            let res_dist = nearest_resistance_distance(snapshot, price);
            let sup_dist = nearest_support_distance(snapshot, price);
            if res_dist <= sup_dist {
                NearLocation::NearResistance
            } else {
                NearLocation::NearSupport
            }
        }
        _ => NearLocation::Clear,
    }
}

fn nearest_resistance_distance(snapshot: &SrSnapshot, price: f64) -> f64 {
    let mut best = f64::INFINITY;
    for zone in snapshot.zones.iter().filter(|z| z.kind == ZoneKind::Resistance) {
        let d = ((zone.low + zone.high) * 0.5 - price).abs();
        if d < best {
            best = d;
        }
    }
    best = best.min((snapshot.daily_high - price).abs());
    best = best.min((snapshot.weekly_high - price).abs());
    best
}

fn nearest_support_distance(snapshot: &SrSnapshot, price: f64) -> f64 {
    let mut best = f64::INFINITY;
    for zone in snapshot.zones.iter().filter(|z| z.kind == ZoneKind::Support) {
        let d = ((zone.low + zone.high) * 0.5 - price).abs();
        if d < best {
            best = d;
        }
    }
    best = best.min((snapshot.daily_low - price).abs());
    best = best.min((snapshot.weekly_low - price).abs());
    best
}

/// Build a full S/R snapshot blending pivots + daily/weekly H/L extremes from
/// the same candle stream.
///
/// `daily_lookback` and `weekly_lookback` are bar counts. Callers passing a
/// 1H stream typically use 24 (D1) and 168 (W1).
pub fn build_sr_snapshot(
    candles: &[Candle],
    pivot_lookback: usize,
    cluster_tolerance: f64,
    max_zones: usize,
    daily_lookback: usize,
    weekly_lookback: usize,
) -> SrSnapshot {
    let zones = detect_zones(candles, pivot_lookback, cluster_tolerance, max_zones);
    let (daily_high, daily_low) = window_extremes(candles, daily_lookback);
    let (weekly_high, weekly_low) = window_extremes(candles, weekly_lookback);
    SrSnapshot {
        zones,
        daily_high,
        daily_low,
        weekly_high,
        weekly_low,
    }
}

fn window_extremes(candles: &[Candle], lookback: usize) -> (f64, f64) {
    if candles.is_empty() {
        return (0.0, 0.0);
    }
    let start = candles.len().saturating_sub(lookback.max(1));
    let slice = &candles[start..];
    let mut hi = f64::MIN;
    let mut lo = f64::MAX;
    for c in slice {
        if c.high > hi {
            hi = c.high;
        }
        if c.low < lo {
            lo = c.low;
        }
    }
    (hi, lo)
}

pub fn detect_zones(
    candles: &[Candle],
    lookback: usize,
    cluster_tolerance: f64,
    max_zones: usize,
) -> Vec<PriceZone> {
    if candles.len() < 5 || max_zones == 0 {
        return Vec::new();
    }

    let start = candles.len().saturating_sub(lookback);
    let slice = &candles[start..];
    let cluster_tolerance = cluster_tolerance.max(f64::EPSILON);
    let mut zones = Vec::new();

    for idx in 2..slice.len().saturating_sub(2) {
        let candle = &slice[idx];
        if is_pivot_low(slice, idx) {
            add_zone(&mut zones, ZoneKind::Support, candle.low, cluster_tolerance);
        }
        if is_pivot_high(slice, idx) {
            add_zone(&mut zones, ZoneKind::Resistance, candle.high, cluster_tolerance);
        }
    }

    zones.sort_by(|left, right| right.strength.total_cmp(&left.strength));
    zones.truncate(max_zones);
    zones
}

fn is_pivot_high(candles: &[Candle], idx: usize) -> bool {
    let value = candles[idx].high;
    candles[idx - 2..=idx + 2]
        .iter()
        .enumerate()
        .all(|(offset, candle)| offset == 2 || value >= candle.high)
}

fn is_pivot_low(candles: &[Candle], idx: usize) -> bool {
    let value = candles[idx].low;
    candles[idx - 2..=idx + 2]
        .iter()
        .enumerate()
        .all(|(offset, candle)| offset == 2 || value <= candle.low)
}

fn add_zone(zones: &mut Vec<PriceZone>, kind: ZoneKind, price: f64, cluster_tolerance: f64) {
    if let Some(zone) = zones.iter_mut().find(|zone| {
        zone.kind == kind
            && price >= zone.low - cluster_tolerance
            && price <= zone.high + cluster_tolerance
    }) {
        // Grow the zone by the actual pivot price, not by the cluster tolerance.
        // A single pivot has zero width; clustered pivots span only the observed extremes.
        zone.low = zone.low.min(price);
        zone.high = zone.high.max(price);
        zone.strength += 1.0;
    } else {
        zones.push(PriceZone {
            kind,
            low: price,
            high: price,
            strength: 1.0,
        });
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    #[test]
    fn clusters_repeated_pivots_into_support_and_resistance_zones() {
        let now = Utc::now();
        let highs = [10.0, 11.0, 15.0, 11.0, 10.0, 12.0, 15.1, 11.0, 10.0];
        let lows = [8.0, 7.0, 6.0, 7.0, 8.0, 7.0, 6.1, 7.0, 8.0];
        let candles = highs
            .into_iter()
            .zip(lows)
            .enumerate()
            .map(|(idx, (high, low))| Candle {
                ts: now + Duration::minutes(idx as i64),
                open: (high + low) / 2.0,
                high,
                low,
                close: (high + low) / 2.0,
                volume: 1_000.0,
            })
            .collect::<Vec<_>>();

        let zones = detect_zones(&candles, 9, 0.2, 4);
        assert!(zones
            .iter()
            .any(|zone| zone.kind == ZoneKind::Resistance && zone.strength >= 2.0));
        assert!(zones
            .iter()
            .any(|zone| zone.kind == ZoneKind::Support && zone.strength >= 2.0));
    }

    #[test]
    fn snapshot_blends_pivot_and_window_extremes() {
        let now = Utc::now();
        let highs = [10.0, 11.0, 15.0, 11.0, 10.0, 12.0, 15.1, 11.0, 10.0];
        let lows = [8.0, 7.0, 6.0, 7.0, 8.0, 7.0, 6.1, 7.0, 8.0];
        let candles = highs
            .into_iter()
            .zip(lows)
            .enumerate()
            .map(|(idx, (high, low))| Candle {
                ts: now + Duration::minutes(idx as i64),
                open: (high + low) / 2.0,
                high,
                low,
                close: (high + low) / 2.0,
                volume: 1_000.0,
            })
            .collect::<Vec<_>>();

        let snapshot = build_sr_snapshot(&candles, 9, 0.2, 4, 5, 9);
        // Last 5 bars window: highs 10,12,15.1,11,10 → 15.1; lows 8,7,6.1,7,8 → 6.1.
        assert_eq!(snapshot.daily_high, 15.1);
        assert_eq!(snapshot.daily_low, 6.1);
        // Full 9-bar window: hi=15.1, lo=6.0.
        assert_eq!(snapshot.weekly_high, 15.1);
        assert_eq!(snapshot.weekly_low, 6.0);
    }

    #[test]
    fn near_status_reports_resistance_when_price_inside_band() {
        let snapshot = SrSnapshot {
            zones: vec![PriceZone {
                kind: ZoneKind::Resistance,
                low: 100.0,
                high: 100.0,
                strength: 2.0,
            }],
            daily_high: 100.0,
            daily_low: 90.0,
            weekly_high: 105.0,
            weekly_low: 85.0,
        };
        // ATR=1, near_atr=0.5 → band=0.5; price 99.7 is within 0.5 of 100.0.
        assert_eq!(
            near_status(&snapshot, 99.7, 1.0, 0.5),
            NearLocation::NearResistance
        );
        assert_eq!(
            near_status(&snapshot, 95.0, 1.0, 0.5),
            NearLocation::Clear
        );
        assert_eq!(
            near_status(&snapshot, 89.95, 1.0, 0.5),
            NearLocation::NearSupport
        );
    }
}
