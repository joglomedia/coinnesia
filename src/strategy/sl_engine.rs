use serde::{Deserialize, Serialize};

use crate::{
    config::{EntryPlanConfig, TrapGuardConfig},
    Candle,
};

use super::{
    plan_context::{f_session_extra, max_sl_distance_for, PlanContext},
    SignalDirection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SlWidth {
    Normal,
    Wide,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StopLoss {
    pub price: f64,
    pub atr_distance: f64,
    /// V61.6 dynamic max-SL cap classification — set to `Wide` when the raw
    /// SL distance exceeds the session cap (the setup is rejected upstream).
    pub width: SlWidth,
    /// True when the structural SL exceeded `max_sl_{session}_atr` and the
    /// setup must be rejected. The price field is still populated so the
    /// panel can render the *would-be* stop for diagnostic display.
    pub rejected_too_wide: bool,
}

impl StopLoss {
    /// Legacy ATR-only constructor kept for callers that don't yet have a
    /// full `PlanContext` (e.g. low-information backtest paths). New code
    /// should prefer [`StopLoss::from_context`].
    pub fn from_atr(
        direction: SignalDirection,
        anchor: f64,
        atr: f64,
        config: &EntryPlanConfig,
    ) -> Self {
        let atr_distance = config.min_sl_distance_atr.max(0.0);
        let sign = match direction {
            SignalDirection::Long => -1.0,
            SignalDirection::Short => 1.0,
            SignalDirection::Wait => 0.0,
        };
        let width = if atr_distance >= config.sl_wide_threshold_atr {
            SlWidth::Wide
        } else {
            SlWidth::Normal
        };
        Self {
            price: anchor + sign * atr * atr_distance,
            atr_distance,
            width,
            rejected_too_wide: false,
        }
    }

    /// Pine `f_long_sl` / `f_short_sl` port: composes session/trap/wick/vol
    /// pads, structural stop (swing vs VWAP/EMA), deep-add cap, wick-off
    /// lookback, and the V61.6 dynamic per-session max-SL cap.
    pub fn from_context(
        ctx: &PlanContext,
        candles: &[Candle],
        config: &EntryPlanConfig,
        trap_guard: &TrapGuardConfig,
        deep_add: f64,
        ew1: f64,
        ew3: f64,
    ) -> Self {
        let atr = ctx.atr.max(0.0);
        let sess_pad = atr
            * f_session_extra(
                ctx.session,
                config.sl_extra_asia_atr,
                config.sl_extra_europe_atr,
                config.sl_extra_usa_atr,
            );
        let trap_pad = if ctx.trap_now || ctx.cooldown_active {
            atr * config.sl_trap_extra_atr
        } else {
            0.0
        };
        let wick_trigger_wick = match ctx.direction {
            SignalDirection::Long => ctx.lower_wick,
            SignalDirection::Short => ctx.upper_wick,
            SignalDirection::Wait => 0.0,
        };
        // Pine: `wickPad = lowerWick >= atrVal * wickATRTrap ? atrVal * slWickExtraATR : 0`
        let wick_pad = if atr > 0.0 && wick_trigger_wick >= atr * trap_guard.wick_trap_atr {
            atr * config.sl_wick_extra_atr
        } else {
            0.0
        };
        let vol_pad = if ctx.vol_shock {
            atr * config.sl_vol_extra_atr
        } else {
            0.0
        };
        let pad = sess_pad + trap_pad + wick_pad + vol_pad;

        let (price, base_too_wide) = match ctx.direction {
            SignalDirection::Long => {
                let base_stop = deep_add - atr * config.min_sl_distance_atr;
                let struct_stop = (ctx.swing_low - pad)
                    .min(ctx.vwap.min(ctx.ema_slow) - pad * 0.5);
                let deep_stop = deep_add
                    - atr
                        * (config.min_sl_distance_atr
                            + f_session_extra(
                                ctx.session,
                                config.sl_extra_asia_atr,
                                config.sl_extra_europe_atr,
                                config.sl_extra_usa_atr,
                            ));
                let mut sl = base_stop.min(struct_stop.min(deep_stop));
                // V61.6 wick-off cap (Pine: `wickLongSL` push-out).
                let wick_sl = lowest_low(candles, config.wick_off_lookback)
                    - atr * config.wick_off_buffer_atr;
                sl = sl.min(wick_sl);
                // Pine line 869: `longSL := max(longSL,
                //   close - atr * maxSLDistanceATR * (useGoldEngine ? altSLFactor : 1.0))`
                // Sub-phase 1.7.16 Gap G — apply `alt_sl_factor` so Gold /
                // Altcoin can keep a wider absolute stop under chaotic flow.
                sl = sl.max(ctx.close - atr * config.max_sl_distance_atr * ctx.alt_sl_factor);

                // V61.6 too-wide reject check (Pine `maxSLDynamic`, line 1023).
                // This stays in place because the dynamic per-session cap is
                // used by the upstream "SL WIDE" reject gate. The cap is NOT
                // applied as an extra clamp on `sl` — Pine doesn't do that.
                let raw_distance_atr = if atr > 0.0 { (ew1 - sl) / atr } else { 0.0 };
                let session_cap = max_sl_distance_for(
                    ctx.session,
                    config.max_sl_asia_atr,
                    config.max_sl_europe_atr,
                    config.max_sl_usa_atr,
                ) * ctx.alt_sl_factor;
                let too_wide = raw_distance_atr > session_cap;
                let _ = ew3; // EW3 ± 0.05 clamp removed (sub-phase 1.7.16 Gap G — not in Pine).
                (sl, too_wide)
            }
            SignalDirection::Short => {
                let base_stop = deep_add + atr * config.min_sl_distance_atr;
                let struct_stop = (ctx.swing_high + pad)
                    .max(ctx.vwap.max(ctx.ema_slow) + pad * 0.5);
                let deep_stop = deep_add
                    + atr
                        * (config.min_sl_distance_atr
                            + f_session_extra(
                                ctx.session,
                                config.sl_extra_asia_atr,
                                config.sl_extra_europe_atr,
                                config.sl_extra_usa_atr,
                            ));
                let mut sl = base_stop.max(struct_stop.max(deep_stop));
                let wick_sl = highest_high(candles, config.wick_off_lookback)
                    + atr * config.wick_off_buffer_atr;
                sl = sl.max(wick_sl);
                // Pine line 870 mirror of the long path. Gap G applies
                // `alt_sl_factor` to widen the absolute SHORT-SL cap.
                sl = sl.min(ctx.close + atr * config.max_sl_distance_atr * ctx.alt_sl_factor);

                let raw_distance_atr = if atr > 0.0 { (sl - ew1) / atr } else { 0.0 };
                let session_cap = max_sl_distance_for(
                    ctx.session,
                    config.max_sl_asia_atr,
                    config.max_sl_europe_atr,
                    config.max_sl_usa_atr,
                ) * ctx.alt_sl_factor;
                let too_wide = raw_distance_atr > session_cap;
                let _ = ew3; // see LONG branch comment
                (sl, too_wide)
            }
            SignalDirection::Wait => (ctx.close, false),
        };

        let atr_distance = if atr > 0.0 {
            (price - ctx.close).abs() / atr
        } else {
            config.min_sl_distance_atr
        };
        let width = if atr_distance >= config.sl_wide_threshold_atr {
            SlWidth::Wide
        } else {
            SlWidth::Normal
        };

        Self {
            price,
            atr_distance,
            width,
            rejected_too_wide: base_too_wide,
        }
    }
}

fn lowest_low(candles: &[Candle], lookback: usize) -> f64 {
    if candles.is_empty() {
        return 0.0;
    }
    let start = candles.len().saturating_sub(lookback.max(1));
    candles[start..]
        .iter()
        .map(|c| c.low)
        .fold(f64::INFINITY, f64::min)
}

fn highest_high(candles: &[Candle], lookback: usize) -> f64 {
    if candles.is_empty() {
        return 0.0;
    }
    let start = candles.len().saturating_sub(lookback.max(1));
    candles[start..]
        .iter()
        .map(|c| c.high)
        .fold(f64::NEG_INFINITY, f64::max)
}
