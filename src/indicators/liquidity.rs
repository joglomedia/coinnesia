use crate::Candle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepKind {
    High,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiquiditySweep {
    pub kind: SweepKind,
    pub level: f64,
    pub reclaimed: bool,
}

pub fn detect_liquidity_sweep(
    candles: &[Candle],
    lookback: usize,
    equal_tolerance: f64,
) -> Option<LiquiditySweep> {
    if candles.len() < lookback.max(2) + 1 {
        return None;
    }

    let latest = candles.last().expect("len checked");
    let start = candles.len().saturating_sub(lookback + 1);
    let prior = &candles[start..candles.len() - 1];
    let equal_tolerance = equal_tolerance.max(0.0);

    let high_level = cluster_level(
        prior.iter().map(|candle| candle.high),
        true,
        equal_tolerance,
    );
    let low_level = cluster_level(
        prior.iter().map(|candle| candle.low),
        false,
        equal_tolerance,
    );

    if let Some(level) = high_level {
        if latest.high > level + equal_tolerance {
            return Some(LiquiditySweep {
                kind: SweepKind::High,
                level,
                reclaimed: latest.close < level,
            });
        }
    }

    if let Some(level) = low_level {
        if latest.low < level - equal_tolerance {
            return Some(LiquiditySweep {
                kind: SweepKind::Low,
                level,
                reclaimed: latest.close > level,
            });
        }
    }

    None
}

fn cluster_level(values: impl Iterator<Item = f64>, high: bool, tolerance: f64) -> Option<f64> {
    let mut values = values.collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    if high {
        values.reverse();
    }

    values.iter().copied().find(|candidate| {
        values
            .iter()
            .filter(|value| (**value - *candidate).abs() <= tolerance)
            .count()
            >= 2
    })
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    #[test]
    fn detects_high_sweep_reclaim() {
        let now = Utc::now();
        let mut candles = (0..5)
            .map(|idx| Candle {
                ts: now + Duration::minutes(idx),
                open: 100.0,
                high: 110.0 + if idx % 2 == 0 { 0.03 } else { -0.03 },
                low: 95.0,
                close: 100.0,
                volume: 1_000.0,
            })
            .collect::<Vec<_>>();
        candles.push(Candle {
            ts: now + Duration::minutes(5),
            open: 108.0,
            high: 112.0,
            low: 102.0,
            close: 109.0,
            volume: 2_000.0,
        });

        let sweep = detect_liquidity_sweep(&candles, 5, 0.1).expect("sweep");
        assert_eq!(sweep.kind, SweepKind::High);
        assert!(sweep.reclaimed);
    }
}
