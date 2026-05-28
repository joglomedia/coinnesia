use super::IndicatorPoint;
use crate::Candle;
use chrono::Timelike;
use chrono_tz::Asia::Jakarta;

/// Pine V61.9 session classification used to anchor `asiaVolEma/europeVolEma/usaVolEma`.
/// Mirrors `f_session_from_minutes` in `TV_BTC_V61_9_*.pine.txt:201`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PineSession {
    Asia,
    Europe,
    Usa,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumePoint {
    pub volume: f64,
    pub average: IndicatorPoint,
    pub ratio: IndicatorPoint,
    pub z_score: IndicatorPoint,
    pub session_average: IndicatorPoint,
    pub session_ratio: IndicatorPoint,
    /// Pine V61.9 per-session EMA baseline (`asiaVolEma/europeVolEma/usaVolEma`).
    pub session_ema: IndicatorPoint,
    /// Pine V61.9 per-session deviation EMA (`asiaDevEma/europeDevEma/usaDevEma`).
    pub session_dev_ema: IndicatorPoint,
    /// `volume / session_ema` — Pine `volRatio` against the session-anchored baseline.
    pub session_ema_ratio: IndicatorPoint,
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

    // Pine V61.9 lines 291-307: alpha = 2 / (sessVolLen + 1); separate state per session.
    let alpha = 2.0 / (period as f64 + 1.0);
    let mut asia_vol: Option<f64> = None;
    let mut europe_vol: Option<f64> = None;
    let mut usa_vol: Option<f64> = None;
    let mut asia_dev: Option<f64> = None;
    let mut europe_dev: Option<f64> = None;
    let mut usa_dev: Option<f64> = None;

    for (idx, candle) in candles.iter().enumerate() {
        rolling += candle.volume;
        rolling_sq += candle.volume * candle.volume;
        if idx >= period {
            rolling -= candles[idx - period].volume;
            rolling_sq -= candles[idx - period].volume * candles[idx - period].volume;
        }

        let pine_session = pine_session_for(candle);
        // Pine: x := na(x) ? volume : x + alpha*(volume - x)
        let (vol_state, dev_state) = match pine_session {
            PineSession::Asia => (&mut asia_vol, &mut asia_dev),
            PineSession::Europe => (&mut europe_vol, &mut europe_dev),
            PineSession::Usa => (&mut usa_vol, &mut usa_dev),
        };
        let vol_ema_now = match *vol_state {
            None => {
                *vol_state = Some(candle.volume);
                candle.volume
            }
            Some(prev) => {
                let next = prev + alpha * (candle.volume - prev);
                *vol_state = Some(next);
                next
            }
        };
        let dev_input = (candle.volume - vol_ema_now).abs();
        let dev_ema_now = match *dev_state {
            None => {
                *dev_state = Some(dev_input);
                dev_input
            }
            Some(prev) => {
                let next = prev + alpha * (dev_input - prev);
                *dev_state = Some(next);
                next
            }
        };
        let session_ema_ratio_val = if vol_ema_now > 0.0 {
            candle.volume / vol_ema_now
        } else {
            0.0
        };

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
                session_ema: IndicatorPoint::ready(vol_ema_now),
                session_dev_ema: IndicatorPoint::ready(dev_ema_now),
                session_ema_ratio: IndicatorPoint::ready(session_ema_ratio_val),
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
                session_ema: IndicatorPoint::ready(vol_ema_now),
                session_dev_ema: IndicatorPoint::ready(dev_ema_now),
                session_ema_ratio: IndicatorPoint::ready(session_ema_ratio_val),
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

/// Pine V61.9 `f_session_from_minutes` (TV_BTC line 201): WIB-anchored.
///   ASIA  07:00-14:59
///   EROPA 15:00-20:29
///   USA   else (covers 20:30-06:59 and the cross-midnight window)
fn pine_session_for(candle: &Candle) -> PineSession {
    let local = candle.ts.with_timezone(&Jakarta);
    let minutes = local.hour() * 60 + local.minute();
    if minutes >= 7 * 60 && minutes < 15 * 60 {
        PineSession::Asia
    } else if minutes >= 15 * 60 && minutes < 20 * 60 + 30 {
        PineSession::Europe
    } else {
        PineSession::Usa
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BreakoutInputs {
    /// `eqHighRef = ta.highest(high[1], 20)` from Pine.
    pub eq_high_ref: f64,
    /// `eqLowRef = ta.lowest(low[1], 20)` from Pine.
    pub eq_low_ref: f64,
    pub atr: f64,
    /// Pine `volRatio` — already evaluated against the session-EMA baseline.
    pub vol_ratio: f64,
    /// Pine `clv = (close - low) / (high - low)`, range [0, 1].
    pub clv: f64,
    pub candle: Candle,
}

/// Pine V61.9 line 688:
///   validBullBreakout = close > eqHighRef and close > open and clv >= 0.68
///                       and candleBody >= atr * 0.45 and volRatio >= sessVolBreakoutRatio
pub fn valid_bull_breakout(inputs: &BreakoutInputs, sess_vol_breakout_ratio: f64) -> bool {
    let body = (inputs.candle.close - inputs.candle.open).abs();
    inputs.candle.close > inputs.eq_high_ref
        && inputs.candle.close > inputs.candle.open
        && inputs.clv >= 0.68
        && body >= inputs.atr * 0.45
        && inputs.vol_ratio >= sess_vol_breakout_ratio
}

/// Pine V61.9 line 689 mirror: bear breakout requires close < open, clv ≤ 0.32,
/// and body ≥ 0.45 ATR with the same session vol-ratio gate.
pub fn valid_bear_breakout(inputs: &BreakoutInputs, sess_vol_breakout_ratio: f64) -> bool {
    let body = (inputs.candle.close - inputs.candle.open).abs();
    inputs.candle.close < inputs.eq_low_ref
        && inputs.candle.close < inputs.candle.open
        && inputs.clv <= 0.32
        && body >= inputs.atr * 0.45
        && inputs.vol_ratio >= sess_vol_breakout_ratio
}

/// Convenience: `validBreakout = validBullBreakout or validBearBreakout`.
pub fn valid_breakout(inputs: &BreakoutInputs, sess_vol_breakout_ratio: f64) -> bool {
    valid_bull_breakout(inputs, sess_vol_breakout_ratio)
        || valid_bear_breakout(inputs, sess_vol_breakout_ratio)
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

    #[test]
    fn session_ema_seeds_to_first_volume_then_smooths() {
        // Build a series where every bar lands in the same Pine session window
        // by spacing them 1 minute apart from a midday WIB anchor.
        let anchor = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 5, 20, 5, 0, 0)
            .unwrap(); // 12:00 WIB → ASIA per Pine
        let candles: Vec<Candle> = [1_000.0, 1_500.0, 2_000.0]
            .into_iter()
            .enumerate()
            .map(|(idx, volume)| Candle {
                ts: anchor + Duration::minutes(idx as i64),
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.5,
                volume,
            })
            .collect();
        let v = volume_engine(&candles, 3);
        // Bar 0 seeds to its own volume → 1000.
        assert_relative_eq!(v[0].session_ema.value, 1_000.0, epsilon = 1e-10);
        // Pine: alpha = 2/(3+1) = 0.5; bar1: 1000 + 0.5*(1500-1000) = 1250.
        assert_relative_eq!(v[1].session_ema.value, 1_250.0, epsilon = 1e-10);
        // bar2: 1250 + 0.5*(2000-1250) = 1625.
        assert_relative_eq!(v[2].session_ema.value, 1_625.0, epsilon = 1e-10);
        // session_ema_ratio = volume / session_ema; bar2 = 2000/1625.
        assert_relative_eq!(
            v[2].session_ema_ratio.value,
            2_000.0 / 1_625.0,
            epsilon = 1e-10
        );
    }

    #[test]
    fn valid_bull_breakout_requires_close_body_and_session_volume() {
        let candle = Candle {
            ts: Utc::now(),
            open: 100.0,
            high: 102.0,
            low: 99.5,
            close: 102.0, // close == high → clv = 1.0
            volume: 5_000.0,
        };
        let inputs = BreakoutInputs {
            eq_high_ref: 101.0,
            eq_low_ref: 99.0,
            atr: 2.0,
            // body = 2.0; 0.45 * atr = 0.9; body ≥ 0.9 ✓
            vol_ratio: 1.30,
            clv: 1.0,
            candle,
        };
        assert!(valid_bull_breakout(&inputs, 1.15));
        // Drop vol below ratio gate → reject.
        let mut weak = inputs.clone();
        weak.vol_ratio = 1.10;
        assert!(!valid_bull_breakout(&weak, 1.15));
        // Bear cannot trigger on bull bar.
        assert!(!valid_bear_breakout(&inputs, 1.15));
    }

    #[test]
    fn valid_bear_breakout_mirrors_bull() {
        let candle = Candle {
            ts: Utc::now(),
            open: 100.0,
            high: 100.5,
            low: 97.0,
            close: 97.0, // clv = 0.0
            volume: 5_000.0,
        };
        let inputs = BreakoutInputs {
            eq_high_ref: 101.0,
            eq_low_ref: 98.0,
            atr: 2.0,
            vol_ratio: 1.50,
            clv: 0.0,
            candle,
        };
        assert!(valid_bear_breakout(&inputs, 1.15));
    }
}
