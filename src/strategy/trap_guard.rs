//! Pine V61.9 trap / manipulation engine port.
//!
//! Mirrors the `trapScoreRaw` formula plus the directional manipulation gate
//! (`longManipulationBlock` / `shortManipulationBlock`), pressure-cluster
//! guard (V61.4), and stealth distribution / accumulation (V61.6) from
//! `docs/TV_Pine_Scripts/TV_BTC_V61_9_FLOW_INTERPRETATION_PANEL_FULL.pine.txt`.

use crate::{
    config::TrapGuardConfig,
    indicators::{
        atr::calculate_atr,
        candle::analyze,
        ema::calculate_ema,
        liquidity::{detect_liquidity_sweep, SweepKind},
        rsi::calculate_rsi,
        volume::volume_engine,
    },
    Candle,
};

use super::SignalDirection;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrapDecision {
    pub blocks_signal: bool,
    pub penalty: f64,
    /// Pine `flowTrapBlock` — set when thin-flow wick risk coincides with a
    /// trap / sweep / volShock event. Consumers tighten TP caps and reduce
    /// probability when this is true even if `blocks_signal` is false.
    pub flow_trap_block: bool,
    /// Pine `trapType` panel label — `"BERSIH"` when no trap fires.
    pub trap_type: TrapType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapType {
    Clean,
    BullTrap,
    BearTrap,
    LiqSweep,
    Distribution,
    Accumulation,
    SlowHunt,
    StopHunt,
}

impl TrapType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Clean => "BERSIH",
            Self::BullTrap => "BULL TRAP",
            Self::BearTrap => "BEAR TRAP",
            Self::LiqSweep => "LIQ SWEEP",
            Self::Distribution => "DISTRIBUSI",
            Self::Accumulation => "AKUMULASI",
            Self::SlowHunt => "SLOW HUNT",
            Self::StopHunt => "STOP HUNT",
        }
    }
}

impl TrapDecision {
    pub const fn allow() -> Self {
        Self {
            blocks_signal: false,
            penalty: 0.0,
            flow_trap_block: false,
            trap_type: TrapType::Clean,
        }
    }
}

pub fn evaluate_trap_guard(
    candles: &[Candle],
    config: &TrapGuardConfig,
    direction: SignalDirection,
) -> TrapDecision {
    if candles.len() < 3 {
        return TrapDecision::allow();
    }

    let latest = candles.last().expect("len checked");
    let shape = analyze(latest);
    let atr = calculate_atr(candles, 14)
        .last()
        .filter(|point| point.ready)
        .map(|point| point.value)
        .unwrap_or_else(|| (latest.high - latest.low).max(0.0));

    let candle_range = (latest.high - latest.low).max(0.0);
    let candle_body = shape.body;

    // Pine `rangeMed = ta.sma(high - low, 20)`.
    let range_med = sma_range(candles, 20);
    let range_shock = candle_range >= range_med * 2.0 && range_med > 0.0;

    let volume = volume_engine(candles, 20);
    let latest_volume = volume.last();
    let vol_z = latest_volume
        .filter(|p| p.z_score.ready)
        .map(|p| p.z_score.value)
        .unwrap_or(0.0);
    let vol_ratio = latest_volume
        .filter(|p| p.ratio.ready)
        .map(|p| p.ratio.value)
        .unwrap_or(1.0);
    let sess_vol_base = latest_volume
        .filter(|p| p.session_average.ready)
        .map(|p| p.session_average.value)
        .filter(|v| *v > 0.0)
        .or_else(|| {
            latest_volume
                .filter(|p| p.average.ready)
                .map(|p| p.average.value)
                .filter(|v| *v > 0.0)
        })
        .unwrap_or(0.0);

    // Pine `wickRatio = max(upperWick, lowerWick) / candleRange`.
    let wick_ratio = if candle_range > 0.0 {
        shape.upper_wick.max(shape.lower_wick) / candle_range
    } else {
        0.0
    };
    // Pine `volShock`.
    let vol_shock =
        range_shock || vol_z >= config.trap_volume_z || wick_ratio >= 0.55;

    // Pine swingHighTrap / swingLowTrap (10-bar lookback on prior bars only).
    let prior = &candles[..candles.len() - 1];
    let lookback10 = prior.len().min(10);
    let swing_high_trap = window_high(prior, lookback10);
    let swing_low_trap = window_low(prior, lookback10);
    // Pine eqHighRef / eqLowRef (20-bar lookback on prior bars).
    let lookback20 = prior.len().min(20);
    let eq_high_ref = window_high(prior, lookback20);
    let eq_low_ref = window_low(prior, lookback20);

    let bull_trap_now = vol_z >= config.trap_volume_z
        && range_shock
        && shape.upper_wick >= atr * config.wick_trap_atr
        && latest.high > swing_high_trap
        && latest.close < swing_high_trap;
    let bear_trap_now = vol_z >= config.trap_volume_z
        && range_shock
        && shape.lower_wick >= atr * config.wick_trap_atr
        && latest.low < swing_low_trap
        && latest.close > swing_low_trap;

    let eq_high_sweep = latest.high > eq_high_ref
        && latest.close < eq_high_ref
        && shape.upper_wick >= atr * 0.12;
    let eq_low_sweep = latest.low < eq_low_ref
        && latest.close > eq_low_ref
        && shape.lower_wick >= atr * 0.12;

    let prev_close = prior.last().map(|c| c.close).unwrap_or(latest.close);
    let slow_stop_hunt = ((latest.close - eq_high_ref).abs() <= atr * 0.35
        && candle_body <= atr * 0.28
        && latest.close > prev_close)
        || ((latest.close - eq_low_ref).abs() <= atr * 0.35
            && candle_body <= atr * 0.28
            && latest.close < prev_close);

    let clv = if candle_range > 0.0 {
        ((latest.close - latest.low) - (latest.high - latest.close)) / candle_range
    } else {
        0.0
    };
    // Pine `validBullBreakout / validBearBreakout`. Pine measures CLV in
    // [0,1]; Coinnesia analyze() produces [-1,1] — convert by /2 + 0.5.
    let clv01 = clv * 0.5 + 0.5;
    let valid_bull_breakout = latest.close > eq_high_ref
        && latest.close > latest.open
        && clv01 >= 0.68
        && candle_body >= atr * 0.45
        && vol_ratio >= config.sess_vol_breakout_ratio;
    let valid_bear_breakout = latest.close < eq_low_ref
        && latest.close < latest.open
        && clv01 <= 0.32
        && candle_body >= atr * 0.45
        && vol_ratio >= config.sess_vol_breakout_ratio;
    let valid_breakout = valid_bull_breakout || valid_bear_breakout;

    // Liquidity-map sweeps (single-bar). These are passed through as
    // `liqSweepHighSMC / liqSweepLowSMC` in Pine.
    let sweep = detect_liquidity_sweep(candles, lookback20.max(1), atr * 0.10);
    let liq_sweep_high = matches!(sweep, Some(s) if s.kind == SweepKind::High);
    let liq_sweep_low = matches!(sweep, Some(s) if s.kind == SweepKind::Low);
    let liq_bull_reclaim = liq_sweep_low && latest.close > latest.open && clv01 >= 0.55;
    let liq_bear_reclaim = liq_sweep_high && latest.close < latest.open && clv01 <= 0.45;

    // Distribution / accumulation rejection counts (V61.6).
    let reject_bars = config.distribution_reject_bars.max(2);
    let rejection_high = rejection_count_high(candles, reject_bars);
    let rejection_low = rejection_count_low(candles, reject_bars);
    let distribution_risk = rejection_high >= 2 || liq_sweep_high;
    let accumulation_risk = rejection_low >= 2 || liq_sweep_low;

    // Pine `trapScoreRaw` — verbatim port.
    let mut trap_score_raw = 0.0_f64;
    if vol_z >= config.trap_volume_z {
        trap_score_raw += 25.0;
    }
    if range_shock {
        trap_score_raw += 20.0;
    }
    if shape.upper_wick >= atr * config.wick_trap_atr
        || shape.lower_wick >= atr * config.wick_trap_atr
    {
        trap_score_raw += 25.0;
    }
    if bull_trap_now || bear_trap_now {
        trap_score_raw += 18.0;
    }
    if eq_high_sweep || eq_low_sweep || liq_sweep_high || liq_sweep_low {
        trap_score_raw += 16.0;
    }
    if slow_stop_hunt {
        trap_score_raw += 16.0;
    }
    if distribution_risk || accumulation_risk {
        trap_score_raw += 10.0;
    }
    if valid_breakout {
        trap_score_raw -= 18.0;
    }
    let trap_score = trap_score_raw.max(0.0);
    let is_trap = trap_score >= config.trap_score_threshold;

    // V61.4 pressure-cluster guard.
    let (bear_pressure, bull_pressure) = pressure_cluster(
        candles,
        config.pressure_cluster_bars,
        config.pressure_cluster_min,
        config.pressure_vol_ratio,
        sess_vol_base,
        atr,
    );

    // V61.6 stealth distribution / accumulation.
    let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
    let rsi = calculate_rsi(&closes, 14);
    let ema_fast = calculate_ema(closes.iter().copied(), 21);
    let ema_fast_now = ema_fast
        .last()
        .filter(|p| p.ready)
        .map(|p| p.value)
        .unwrap_or(latest.close);
    let rsi_now = rsi
        .last()
        .filter(|p| p.ready)
        .map(|p| p.value)
        .unwrap_or(50.0);
    let rsi_prev = rsi
        .iter()
        .rev()
        .nth(1)
        .filter(|p| p.ready)
        .map(|p| p.value)
        .unwrap_or(rsi_now);
    let stealth_distribution = stealth_distribution(candles, 8)
        && latest.close < ema_fast_now
        && (sess_vol_base == 0.0 || latest.volume <= sess_vol_base * 1.20)
        && rsi_now < rsi_prev
        && latest.high <= window_high(candles, 12);
    let stealth_accumulation = stealth_accumulation(candles, 8)
        && latest.close > ema_fast_now
        && (sess_vol_base == 0.0 || latest.volume <= sess_vol_base * 1.20)
        && rsi_now > rsi_prev
        && latest.low >= window_low(candles, 12);

    // Directional manipulation gate.
    let fake_bull_breakout_risk = (liq_sweep_high || eq_high_sweep || bull_trap_now)
        && !valid_bull_breakout;
    let fake_bear_breakout_risk = (liq_sweep_low || eq_low_sweep || bear_trap_now)
        && !valid_bear_breakout;
    let body_high = latest.open.max(latest.close);
    let body_low = latest.open.min(latest.close);
    let fake_vol_long = vol_shock
        && shape.upper_wick > shape.lower_wick
        && latest.close <= body_high
        && latest.close < latest.open;
    let fake_vol_short = vol_shock
        && shape.lower_wick > shape.upper_wick
        && latest.close >= body_low
        && latest.close > latest.open;
    let wick_off_long = shape.upper_wick >= atr * config.wick_trap_atr
        && clv01 <= 0.45
        && latest.high > swing_high_trap;
    let wick_off_short = shape.lower_wick >= atr * config.wick_trap_atr
        && clv01 >= 0.55
        && latest.low < swing_low_trap;

    let long_block = fake_bull_breakout_risk
        || wick_off_long
        || fake_vol_long
        || slow_stop_hunt
        || bear_pressure
        || stealth_distribution;
    let short_block = fake_bear_breakout_risk
        || wick_off_short
        || fake_vol_short
        || slow_stop_hunt
        || bull_pressure
        || stealth_accumulation;

    // Pine `flowTrapBlock` (thin-flow gate). The flow_state classification
    // itself lives in `plan_context::classify_flow`; here we mirror only the
    // wick + trap conjunction so callers can tighten TP caps without
    // re-classifying flow.
    let thin_flow_wick_risk = wick_ratio >= 0.55 && vol_ratio <= 0.85;
    let flow_trap_block = thin_flow_wick_risk
        && (vol_shock
            || liq_sweep_high
            || liq_sweep_low
            || is_trap
            || bull_trap_now
            || bear_trap_now);

    let blocks = match direction {
        SignalDirection::Long => long_block || (is_trap && !liq_bull_reclaim),
        SignalDirection::Short => short_block || (is_trap && !liq_bear_reclaim),
        SignalDirection::Wait => false,
    };

    let trap_type = if bull_trap_now {
        TrapType::BullTrap
    } else if bear_trap_now {
        TrapType::BearTrap
    } else if liq_sweep_high || liq_sweep_low || eq_high_sweep || eq_low_sweep {
        TrapType::LiqSweep
    } else if distribution_risk {
        TrapType::Distribution
    } else if accumulation_risk {
        TrapType::Accumulation
    } else if slow_stop_hunt {
        TrapType::SlowHunt
    } else if is_trap {
        TrapType::StopHunt
    } else {
        TrapType::Clean
    };

    TrapDecision {
        blocks_signal: blocks,
        penalty: trap_score,
        flow_trap_block,
        trap_type,
    }
}

fn sma_range(candles: &[Candle], lookback: usize) -> f64 {
    let n = candles.len().min(lookback);
    if n == 0 {
        return 0.0;
    }
    let start = candles.len() - n;
    let sum: f64 = candles[start..]
        .iter()
        .map(|c| (c.high - c.low).max(0.0))
        .sum();
    sum / n as f64
}

fn window_high(candles: &[Candle], lookback: usize) -> f64 {
    if candles.is_empty() {
        return 0.0;
    }
    let n = candles.len().min(lookback.max(1));
    let start = candles.len() - n;
    candles[start..]
        .iter()
        .map(|c| c.high)
        .fold(f64::NEG_INFINITY, f64::max)
}

fn window_low(candles: &[Candle], lookback: usize) -> f64 {
    if candles.is_empty() {
        return 0.0;
    }
    let n = candles.len().min(lookback.max(1));
    let start = candles.len() - n;
    candles[start..]
        .iter()
        .map(|c| c.low)
        .fold(f64::INFINITY, f64::min)
}

fn rejection_count_high(candles: &[Candle], lookback: usize) -> usize {
    if candles.is_empty() {
        return 0;
    }
    let n = candles.len().min(lookback);
    let start = candles.len() - n;
    let mut count = 0;
    for (idx, c) in candles[start..].iter().enumerate() {
        let abs_idx = start + idx;
        let ref_window_end = abs_idx; // exclusive — Pine's `[1]` shift
        if ref_window_end == 0 {
            continue;
        }
        let ref_start = ref_window_end.saturating_sub(lookback);
        let ref_high = window_high(&candles[ref_start..ref_window_end], lookback);
        let shape = analyze(c);
        if shape.upper_wick > shape.body && c.high >= ref_high {
            count += 1;
        }
    }
    count
}

fn rejection_count_low(candles: &[Candle], lookback: usize) -> usize {
    if candles.is_empty() {
        return 0;
    }
    let n = candles.len().min(lookback);
    let start = candles.len() - n;
    let mut count = 0;
    for (idx, c) in candles[start..].iter().enumerate() {
        let abs_idx = start + idx;
        let ref_window_end = abs_idx;
        if ref_window_end == 0 {
            continue;
        }
        let ref_start = ref_window_end.saturating_sub(lookback);
        let ref_low = window_low(&candles[ref_start..ref_window_end], lookback);
        let shape = analyze(c);
        if shape.lower_wick > shape.body && c.low <= ref_low {
            count += 1;
        }
    }
    count
}

/// Pine V61.4 `bearVolCluster / bullVolCluster + bearPressure / bullPressure`.
/// We keep only the volume-cluster portion here; the EMA/MACD/VWAP filters
/// from Pine are evaluated upstream, so this returns the cluster bool and
/// the caller can AND with structure context. Returns (bear, bull).
fn pressure_cluster(
    candles: &[Candle],
    bars: usize,
    min_count: usize,
    vol_ratio: f64,
    sess_vol_base: f64,
    _atr: f64,
) -> (bool, bool) {
    if candles.is_empty() || sess_vol_base <= 0.0 || bars == 0 {
        return (false, false);
    }
    let n = candles.len().min(bars);
    let start = candles.len() - n;
    let window = &candles[start..];
    let bear = window
        .iter()
        .filter(|c| c.close < c.open && c.volume > sess_vol_base * vol_ratio)
        .count()
        >= min_count;
    let bull = window
        .iter()
        .filter(|c| c.close > c.open && c.volume > sess_vol_base * vol_ratio)
        .count()
        >= min_count;
    (bear, bull)
}

/// Pine `math.sum(upperWick > candleBody ? 1 : 0, 8) >= 4`.
fn stealth_distribution(candles: &[Candle], bars: usize) -> bool {
    let n = candles.len().min(bars);
    if n == 0 {
        return false;
    }
    let start = candles.len() - n;
    candles[start..]
        .iter()
        .filter(|c| {
            let s = analyze(c);
            s.upper_wick > s.body
        })
        .count()
        >= 4
}

fn stealth_accumulation(candles: &[Candle], bars: usize) -> bool {
    let n = candles.len().min(bars);
    if n == 0 {
        return false;
    }
    let start = candles.len() - n;
    candles[start..]
        .iter()
        .filter(|c| {
            let s = analyze(c);
            s.lower_wick > s.body
        })
        .count()
        >= 4
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    fn flat_candles(n: usize) -> Vec<Candle> {
        let now = Utc::now();
        (0..n)
            .map(|idx| Candle {
                ts: now + Duration::minutes(idx as i64),
                open: 100.0,
                high: 102.0,
                low: 98.0,
                close: 100.0,
                volume: 1_000.0,
            })
            .collect()
    }

    #[test]
    fn bull_trap_blocks_long_signal() {
        let now = Utc::now();
        let mut candles = flat_candles(25);
        candles.push(Candle {
            ts: now + Duration::minutes(26),
            open: 101.0,
            high: 120.0,
            low: 100.0,
            close: 102.0,
            volume: 4_000.0,
        });
        let config = crate::config::AppConfig::from_default_toml()
            .expect("default config parses")
            .trap_guard;

        let decision = evaluate_trap_guard(&candles, &config, SignalDirection::Long);
        assert!(decision.blocks_signal);
        assert!(decision.penalty > 0.0);
    }

    #[test]
    fn pressure_cluster_fires_on_bear_run() {
        let now = Utc::now();
        // First 12 baseline bars to establish session_vol_base.
        let mut candles: Vec<Candle> = (0..12)
            .map(|idx| Candle {
                ts: now + Duration::minutes(idx),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0,
                volume: 1_000.0,
            })
            .collect();
        // 8-bar bear cluster, each above pressure_vol_ratio.
        for idx in 0..8 {
            candles.push(Candle {
                ts: now + Duration::minutes(12 + idx),
                open: 100.0,
                high: 100.5,
                low: 98.0,
                close: 99.0,
                volume: 1_500.0,
            });
        }
        let config = crate::config::AppConfig::from_default_toml()
            .unwrap()
            .trap_guard;
        let (bear, bull) = pressure_cluster(
            &candles,
            config.pressure_cluster_bars,
            config.pressure_cluster_min,
            config.pressure_vol_ratio,
            1_000.0,
            1.0,
        );
        assert!(bear, "expected bear cluster to fire");
        assert!(!bull);
    }

    #[test]
    fn stealth_distribution_counts_upper_wick_bars() {
        let now = Utc::now();
        // 8 bars: 5 upper-wick-dominant (no lower wick) and 3 neutral bars
        // with neither wick > body. Only the upper-wick helper should fire.
        let candles: Vec<Candle> = (0..8)
            .map(|idx| {
                if idx < 5 {
                    Candle {
                        ts: now + Duration::minutes(idx),
                        open: 100.0,
                        high: 105.0,
                        low: 100.0,    // no lower wick
                        close: 100.2,  // tiny body, big upper wick
                        volume: 1_000.0,
                    }
                } else {
                    Candle {
                        ts: now + Duration::minutes(idx),
                        open: 100.0,
                        high: 101.0,
                        low: 99.0,
                        close: 102.0, // body=2 > wicks
                        volume: 1_000.0,
                    }
                }
            })
            .collect();
        assert!(stealth_distribution(&candles, 8));
        assert!(!stealth_accumulation(&candles, 8));
    }

    #[test]
    fn flow_trap_block_set_when_thin_flow_meets_trap() {
        // Construct a high-wick, low-volume bar after a quiet baseline so
        // wickRatio>=0.55 and volRatio<=0.85 → thinFlowWickRisk; combined
        // with bull_trap conditions it should set flow_trap_block.
        let now = Utc::now();
        let mut candles = flat_candles(25);
        candles.push(Candle {
            ts: now + Duration::minutes(26),
            open: 100.5,
            high: 110.0,
            low: 100.0,
            close: 100.7, // tiny body, huge upper wick
            volume: 600.0, // below baseline → low vol_ratio
        });
        let config = crate::config::AppConfig::from_default_toml()
            .unwrap()
            .trap_guard;
        let decision = evaluate_trap_guard(&candles, &config, SignalDirection::Long);
        assert!(
            decision.flow_trap_block,
            "thin-flow wick risk + trap-shaped bar should set flow_trap_block"
        );
    }

    #[test]
    fn clean_baseline_returns_clean_label() {
        // Build a quiet uptrend with body-dominated bars (no wicks) so none
        // of trap, sweep, distribution, accumulation, or stealth fires.
        let now = Utc::now();
        let candles: Vec<Candle> = (0..30)
            .map(|idx| {
                let base = 100.0 + idx as f64 * 0.10;
                Candle {
                    ts: now + Duration::minutes(idx),
                    open: base,
                    high: base + 0.30,
                    low: base,
                    close: base + 0.30,
                    volume: 1_000.0,
                }
            })
            .collect();
        let config = crate::config::AppConfig::from_default_toml()
            .unwrap()
            .trap_guard;
        let decision = evaluate_trap_guard(&candles, &config, SignalDirection::Long);
        assert_eq!(decision.trap_type, TrapType::Clean);
        assert!(!decision.blocks_signal);
    }
}
