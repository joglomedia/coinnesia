use crate::Candle;

use super::IndicatorPoint;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelativeStrengthPoint {
    /// `(close / close[len] - 1) * 100` for the asset.
    pub asset_perf: IndicatorPoint,
    /// Same formula applied to the benchmark series.
    pub benchmark_perf: IndicatorPoint,
    /// `asset_perf - benchmark_perf` (Pine `rsVsIdx`).
    pub rs: IndicatorPoint,
}

impl RelativeStrengthPoint {
    /// Pine `rsOK = rsVsIdx > 0`.
    pub fn rs_ok(&self) -> bool {
        self.rs.ready && self.rs.value > 0.0
    }
}

/// Pine V5 IDX:
///   stockPerf = close[len] != 0 ? (close / close[len] - 1.0) * 100.0 : na
///   idxPerf   = idxClose[len] != 0 ? (idxClose / idxClose[len] - 1.0) * 100.0 : na
///   rsVsIdx   = stockPerf - idxPerf
///   rsOK      = rsVsIdx > 0
///
/// `length` defaults to 10. The benchmark series must align bar-for-bar with
/// `asset` — callers should resample at the same timeframe before calling.
pub fn calculate_relative_strength(
    asset: &[Candle],
    benchmark: &[Candle],
    length: usize,
) -> Vec<RelativeStrengthPoint> {
    assert!(length > 0, "RS length must be greater than zero");
    let n = asset.len().min(benchmark.len());
    let mut out = Vec::with_capacity(n);

    for idx in 0..n {
        if idx < length {
            out.push(RelativeStrengthPoint {
                asset_perf: IndicatorPoint::pending(),
                benchmark_perf: IndicatorPoint::pending(),
                rs: IndicatorPoint::pending(),
            });
            continue;
        }
        let asset_prev = asset[idx - length].close;
        let bench_prev = benchmark[idx - length].close;
        let asset_perf = if asset_prev != 0.0 {
            Some((asset[idx].close / asset_prev - 1.0) * 100.0)
        } else {
            None
        };
        let bench_perf = if bench_prev != 0.0 {
            Some((benchmark[idx].close / bench_prev - 1.0) * 100.0)
        } else {
            None
        };

        let asset_pt = match asset_perf {
            Some(v) => IndicatorPoint::ready(v),
            None => IndicatorPoint::pending(),
        };
        let bench_pt = match bench_perf {
            Some(v) => IndicatorPoint::ready(v),
            None => IndicatorPoint::pending(),
        };
        let rs_pt = match (asset_perf, bench_perf) {
            (Some(a), Some(b)) => IndicatorPoint::ready(a - b),
            _ => IndicatorPoint::pending(),
        };

        out.push(RelativeStrengthPoint {
            asset_perf: asset_pt,
            benchmark_perf: bench_pt,
            rs: rs_pt,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use approx::assert_relative_eq;
    use chrono::{Duration, Utc};

    use super::*;

    fn series(starts: &[f64]) -> Vec<Candle> {
        let now = Utc::now();
        starts
            .iter()
            .enumerate()
            .map(|(idx, &close)| Candle {
                ts: now + Duration::days(idx as i64),
                open: close,
                high: close + 0.5,
                low: close - 0.5,
                close,
                volume: 1_000.0,
            })
            .collect()
    }

    #[test]
    fn rs_pending_until_lookback_satisfied() {
        let a = series(&[100.0, 101.0, 102.0, 103.0, 104.0]);
        let b = series(&[200.0, 201.0, 202.0, 203.0, 204.0]);
        let rs = calculate_relative_strength(&a, &b, 3);
        assert!(!rs[2].rs.ready);
        assert!(rs[3].rs.ready);
    }

    #[test]
    fn rs_positive_when_asset_outperforms() {
        // 10% gain vs 5% gain over 10 bars → +5.0 RS.
        let mut a_closes = vec![100.0; 11];
        let mut b_closes = vec![100.0; 11];
        a_closes[10] = 110.0; // +10%
        b_closes[10] = 105.0; // +5%
        let a = series(&a_closes);
        let b = series(&b_closes);
        let rs = calculate_relative_strength(&a, &b, 10);
        let last = rs.last().unwrap();
        assert!(last.rs_ok());
        assert_relative_eq!(last.rs.value, 5.0, epsilon = 1e-9);
    }

    #[test]
    fn rs_negative_when_asset_underperforms() {
        let mut a_closes = vec![100.0; 11];
        let mut b_closes = vec![100.0; 11];
        a_closes[10] = 102.0;
        b_closes[10] = 110.0;
        let a = series(&a_closes);
        let b = series(&b_closes);
        let rs = calculate_relative_strength(&a, &b, 10);
        let last = rs.last().unwrap();
        assert!(!last.rs_ok());
        assert!(last.rs.value < 0.0);
    }
}
