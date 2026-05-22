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

pub fn detect_zones(
    candles: &[Candle],
    lookback: usize,
    tolerance: f64,
    max_zones: usize,
) -> Vec<PriceZone> {
    if candles.len() < 5 || max_zones == 0 {
        return Vec::new();
    }

    let start = candles.len().saturating_sub(lookback);
    let slice = &candles[start..];
    let tolerance = tolerance.max(f64::EPSILON);
    let mut zones = Vec::new();

    for idx in 2..slice.len().saturating_sub(2) {
        let candle = &slice[idx];
        if is_pivot_low(slice, idx) {
            add_zone(&mut zones, ZoneKind::Support, candle.low, tolerance);
        }
        if is_pivot_high(slice, idx) {
            add_zone(&mut zones, ZoneKind::Resistance, candle.high, tolerance);
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

fn add_zone(zones: &mut Vec<PriceZone>, kind: ZoneKind, price: f64, tolerance: f64) {
    if let Some(zone) = zones.iter_mut().find(|zone| {
        zone.kind == kind && price >= zone.low - tolerance && price <= zone.high + tolerance
    }) {
        zone.low = zone.low.min(price - tolerance);
        zone.high = zone.high.max(price + tolerance);
        zone.strength += 1.0;
    } else {
        zones.push(PriceZone {
            kind,
            low: price - tolerance,
            high: price + tolerance,
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
}
