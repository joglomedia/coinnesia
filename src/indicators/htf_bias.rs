use serde::{Deserialize, Serialize};

use crate::Candle;

use super::ema::calculate_ema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BiasVote {
    Long,
    Short,
    Neutral,
}

impl BiasVote {
    pub const fn score(self) -> i32 {
        match self {
            BiasVote::Long => 1,
            BiasVote::Short => -1,
            BiasVote::Neutral => 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BiasFrame {
    pub vote: BiasVote,
    pub close: f64,
    pub ema_fast: f64,
    pub ema_mid: f64,
    pub ema_trend: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct HtfBiasResult {
    pub h4: Option<BiasFrame>,
    pub d1: Option<BiasFrame>,
    /// `htfLongScore`: count of frames voting long.
    pub long_score: u32,
    /// `htfShortScore`: count of frames voting short.
    pub short_score: u32,
    /// `htfMaxScore`: total enabled frames.
    pub max_score: u32,
    /// Pine `htfBlockLong = blockCounterHTF and htfMaxScore > 0 and htfShortScore == htfMaxScore`.
    pub block_long: bool,
    /// Mirror of `htfBlockShort`.
    pub block_short: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct HtfBiasConfig {
    pub ema_fast: usize,
    pub ema_mid: usize,
    pub ema_trend: usize,
}

impl HtfBiasConfig {
    pub const fn forex_default() -> Self {
        Self {
            ema_fast: 21,
            ema_mid: 55,
            ema_trend: 200,
        }
    }
}

/// Pine V58 Forex bias rule (`f_bias`):
///   c > eFast > eMid > eTrend           → +1
///   c < eFast < eMid < eTrend           → −1
///   c > eMid > eTrend (relaxed)         → +1
///   c < eMid < eTrend (relaxed)         → −1
///   else                                → 0
pub fn classify_bias(close: f64, e_fast: f64, e_mid: f64, e_trend: f64) -> BiasVote {
    if close > e_fast && e_fast > e_mid && e_mid > e_trend {
        BiasVote::Long
    } else if close < e_fast && e_fast < e_mid && e_mid < e_trend {
        BiasVote::Short
    } else if close > e_mid && e_mid > e_trend {
        BiasVote::Long
    } else if close < e_mid && e_mid < e_trend {
        BiasVote::Short
    } else {
        BiasVote::Neutral
    }
}

/// Compute one HTF frame's bias from its candle stream.
///
/// Returns `None` when there isn't enough history to seed `ema_trend`.
pub fn frame_bias(candles: &[Candle], config: HtfBiasConfig) -> Option<BiasFrame> {
    if candles.is_empty() {
        return None;
    }
    let closes = candles.iter().map(|c| c.close).collect::<Vec<_>>();
    let fast = calculate_ema(closes.iter().copied(), config.ema_fast);
    let mid = calculate_ema(closes.iter().copied(), config.ema_mid);
    let trend = calculate_ema(closes.iter().copied(), config.ema_trend);
    let last = closes.len() - 1;
    let f = fast.get(last)?;
    let m = mid.get(last)?;
    let t = trend.get(last)?;
    if !(f.ready && m.ready && t.ready) {
        return None;
    }
    let close = *closes.last()?;
    Some(BiasFrame {
        vote: classify_bias(close, f.value, m.value, t.value),
        close,
        ema_fast: f.value,
        ema_mid: m.value,
        ema_trend: t.value,
    })
}

/// Aggregate H4 + D1 bias votes per Pine V58 Forex.
///
/// `block_counter_htf` is the Pine `blockCounterHTF` boolean. Pass either H4 or
/// D1 candles as `None` to disable that frame (matching Pine's `useHTF1/useHTF2`).
pub fn aggregate_htf_bias(
    h4: Option<&[Candle]>,
    d1: Option<&[Candle]>,
    config: HtfBiasConfig,
    block_counter_htf: bool,
) -> HtfBiasResult {
    let h4_frame = h4.and_then(|c| frame_bias(c, config));
    let d1_frame = d1.and_then(|c| frame_bias(c, config));

    let mut long_score: u32 = 0;
    let mut short_score: u32 = 0;
    let mut max_score: u32 = 0;
    for frame in [h4_frame, d1_frame].into_iter().flatten() {
        max_score += 1;
        match frame.vote {
            BiasVote::Long => long_score += 1,
            BiasVote::Short => short_score += 1,
            BiasVote::Neutral => {}
        }
    }

    let block_long = block_counter_htf && max_score > 0 && short_score == max_score;
    let block_short = block_counter_htf && max_score > 0 && long_score == max_score;

    HtfBiasResult {
        h4: h4_frame,
        d1: d1_frame,
        long_score,
        short_score,
        max_score,
        block_long,
        block_short,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    fn rising_candles(n: usize, slope: f64) -> Vec<Candle> {
        let now = Utc::now();
        (0..n)
            .map(|idx| {
                let close = 100.0 + idx as f64 * slope;
                Candle {
                    ts: now + Duration::hours(idx as i64),
                    open: close,
                    high: close + 0.5,
                    low: close - 0.5,
                    close,
                    volume: 1_000.0,
                }
            })
            .collect()
    }

    fn falling_candles(n: usize, slope: f64) -> Vec<Candle> {
        let now = Utc::now();
        (0..n)
            .map(|idx| {
                let close = 1_000.0 - idx as f64 * slope;
                Candle {
                    ts: now + Duration::hours(idx as i64),
                    open: close,
                    high: close + 0.5,
                    low: close - 0.5,
                    close,
                    volume: 1_000.0,
                }
            })
            .collect()
    }

    #[test]
    fn classify_bias_strict_long() {
        // c > eFast > eMid > eTrend → Long.
        assert_eq!(classify_bias(110.0, 105.0, 100.0, 95.0), BiasVote::Long);
        // c < eFast < eMid < eTrend → Short.
        assert_eq!(classify_bias(90.0, 95.0, 100.0, 105.0), BiasVote::Short);
    }

    #[test]
    fn classify_bias_relaxed_paths() {
        // c > eMid > eTrend even if fast scrambled → Long.
        assert_eq!(classify_bias(102.0, 99.0, 100.0, 95.0), BiasVote::Long);
        assert_eq!(classify_bias(98.0, 101.0, 100.0, 105.0), BiasVote::Short);
    }

    #[test]
    fn classify_bias_neutral_when_no_alignment() {
        assert_eq!(classify_bias(100.0, 100.0, 100.0, 100.0), BiasVote::Neutral);
    }

    #[test]
    fn aggregate_with_two_long_frames_blocks_short_only() {
        let h4 = rising_candles(250, 1.0);
        let d1 = rising_candles(250, 1.0);
        let r = aggregate_htf_bias(
            Some(&h4),
            Some(&d1),
            HtfBiasConfig::forex_default(),
            true,
        );
        assert_eq!(r.long_score, 2);
        assert_eq!(r.short_score, 0);
        assert_eq!(r.max_score, 2);
        assert!(r.block_short, "two long frames must block short");
        assert!(!r.block_long);
    }

    #[test]
    fn aggregate_with_two_short_frames_blocks_long_only() {
        let h4 = falling_candles(250, 1.0);
        let d1 = falling_candles(250, 1.0);
        let r = aggregate_htf_bias(
            Some(&h4),
            Some(&d1),
            HtfBiasConfig::forex_default(),
            true,
        );
        assert!(r.block_long);
        assert!(!r.block_short);
    }

    #[test]
    fn aggregate_with_block_disabled_emits_no_blocks() {
        let h4 = falling_candles(250, 1.0);
        let d1 = falling_candles(250, 1.0);
        let r = aggregate_htf_bias(
            Some(&h4),
            Some(&d1),
            HtfBiasConfig::forex_default(),
            false,
        );
        assert!(!r.block_long);
        assert!(!r.block_short);
    }
}
