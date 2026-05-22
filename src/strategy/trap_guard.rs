use crate::{
    config::TrapGuardConfig,
    indicators::{
        atr::calculate_atr,
        candle::analyze,
        liquidity::{detect_liquidity_sweep, SweepKind},
        volume::volume_engine,
    },
    Candle,
};

use super::SignalDirection;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrapDecision {
    pub blocks_signal: bool,
    pub penalty: f64,
}

impl TrapDecision {
    pub const fn allow() -> Self {
        Self {
            blocks_signal: false,
            penalty: 0.0,
        }
    }

    pub const fn block(penalty: f64) -> Self {
        Self {
            blocks_signal: true,
            penalty,
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
    let volume = volume_engine(candles, 20);
    let latest_volume = volume.last();
    let sweep = detect_liquidity_sweep(candles, 20.min(candles.len() - 1), atr * 0.08);

    let mut trap_score = 0.0;
    if atr > 0.0 && (latest.high - latest.low) / atr >= 3.0 {
        trap_score += 100.0;
    }
    if latest_volume
        .filter(|point| point.z_score.ready && point.z_score.value >= config.trap_volume_z)
        .is_some()
    {
        trap_score += 20.0;
    }

    match direction {
        SignalDirection::Long => {
            if shape.upper_wick >= atr * config.wick_trap_atr {
                trap_score += 35.0;
            }
            if matches!(
                sweep,
                Some(sweep) if sweep.kind == SweepKind::High && sweep.reclaimed
            ) {
                trap_score += 45.0;
            }
        }
        SignalDirection::Short => {
            if shape.lower_wick >= atr * config.wick_trap_atr {
                trap_score += 35.0;
            }
            if matches!(
                sweep,
                Some(sweep) if sweep.kind == SweepKind::Low && sweep.reclaimed
            ) {
                trap_score += 45.0;
            }
        }
        SignalDirection::Wait => {}
    }

    if trap_score >= config.trap_score_threshold {
        TrapDecision::block(trap_score)
    } else {
        TrapDecision {
            blocks_signal: false,
            penalty: trap_score,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    #[test]
    fn bull_trap_blocks_long_signal() {
        let now = Utc::now();
        let mut candles = (0..25)
            .map(|idx| Candle {
                ts: now + Duration::minutes(idx),
                open: 100.0,
                high: 102.0,
                low: 98.0,
                close: 100.0,
                volume: 1_000.0,
            })
            .collect::<Vec<_>>();
        candles.push(Candle {
            ts: now + Duration::minutes(26),
            open: 101.0,
            high: 120.0,
            low: 100.0,
            close: 102.0,
            volume: 4_000.0,
        });
        let config = TrapGuardConfig {
            trap_score_threshold: 60.0,
            trap_volume_z: 2.0,
            wick_trap_atr: 0.7,
            cooldown_bars: 3,
        };

        assert!(evaluate_trap_guard(&candles, &config, SignalDirection::Long).blocks_signal);
    }
}
