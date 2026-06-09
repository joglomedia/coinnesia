use crate::{
    assets::AssetEvaluator,
    config::AppConfig,
    strategy::{
        plan_context::{f_clamp, FlowState},
        session::MarketSession,
        signals::IndicatorSnapshot,
        SignalDirection,
    },
    AssetClass, Timeframe,
};

const ALT_LTF_WEIGHT: f64 = 1.25;
const ALT_HTF_RELAX: f64 = 0.18;

/// Pine V62.0 altcoin philosophy: anti-trap first. The full V62 adaptive
/// engine (AUTO/MAJOR/MID/MEME profile resolution + altChaos no-trade) lands
/// in the V62-specific config bucket — the work is tracked in
/// `[assets.altcoin]` of `docs/phase1_pine_parity_plan.md` §1.7.12. For now
/// the altcoin evaluator extends the BTC default with one Pine-derived hard
/// gate: when the M1+M5+M15 LTF triad is unanimously against `direction`,
/// reject regardless of how the additive score landed. This mirrors V62's
/// `altChaos` short-circuit where micro-frame disagreement nukes any setup.
pub const PHILOSOPHY: &str = "anti_trap_first";

pub struct AltcoinEvaluator;

impl AssetEvaluator for AltcoinEvaluator {
    fn asset_class(&self) -> AssetClass {
        AssetClass::Altcoin
    }

    fn extra_gate(
        &self,
        direction: SignalDirection,
        snapshot: &IndicatorSnapshot<'_>,
        _config: &AppConfig,
    ) -> Option<String> {
        let triad = [Timeframe::M1, Timeframe::M5, Timeframe::M15];
        let opposite = match direction {
            SignalDirection::Long => SignalDirection::Short,
            SignalDirection::Short => SignalDirection::Long,
            SignalDirection::Wait => return None,
        };
        let votes = triad
            .iter()
            .filter_map(|tf| snapshot.mtf.get(tf))
            .map(|summary| summary.bias)
            .collect::<Vec<_>>();
        if votes.is_empty() {
            return None;
        }
        if votes.iter().all(|bias| *bias == opposite) {
            return Some(format!(
                "altcoin_chaos_block:{:?}",
                direction
            ));
        }
        None
    }

    /// Pine V62 line 388:
    /// ```text
    /// altEWFactor = f_clamp(
    ///     (altWildVol ? altEWVolCompress : 1.0)
    ///   * (altThinFlow ? altEWThinCompress : 1.0)
    ///   * (sess == "ASIA"  ? 0.92
    ///      : sess == "EROPA" ? 0.98
    ///      : 1.02),                                  // USA / else
    ///     0.42, 1.08)
    /// ```
    /// Same wildVol/thinFlow proxies as Gold; the Asia leg drops to 0.92 and
    /// USA boosts to 1.02 (Pine treats USA as the highest-flow session for
    /// majors/mids). Token-specific `altProfile` knobs remain a 1.7.16 Gap F
    /// follow-up.
    fn ew_compression_factor(
        &self,
        session: MarketSession,
        flow: FlowState,
        shock_active: bool,
    ) -> f64 {
        const ALT_EW_VOL_COMPRESS: f64 = 0.86;
        const ALT_EW_THIN_COMPRESS: f64 = 0.90;
        let alt_wild_vol = shock_active;
        let alt_thin_flow = matches!(flow, FlowState::Low);
        let vol_mult = if alt_wild_vol { ALT_EW_VOL_COMPRESS } else { 1.0 };
        let flow_mult = if alt_thin_flow { ALT_EW_THIN_COMPRESS } else { 1.0 };
        let session_mult = match session {
            MarketSession::Asia => 0.92,
            MarketSession::Europe => 0.98,
            _ => 1.02, // USA, Idx, RolloverAvoid all map to Pine's else-branch
        };
        f_clamp(vol_mult * flow_mult * session_mult, 0.42, 1.08)
    }

    /// Pine V62 line 390:
    /// ```text
    /// altSLFactor = f_clamp(
    ///     1.0
    ///   + (altWickChaos ? altSLWickBufferATR : 0.0)   // +0.28 if wick chaos
    ///   + (altWildVol   ? altSLVolBufferATR  : 0.0)   // +0.18 if vol shock
    ///   + (altThinFlow  ? 0.08               : 0.0),
    ///     1.0, 1.85)
    /// ```
    /// Altcoin allows a noticeably wider SL than Gold (`1.85` ceiling vs
    /// `1.55`) because alt majors and mids commonly run wider stops in
    /// fast-flow phases. `altWickChaos` is token-specific and skipped here.
    fn sl_extension_factor(
        &self,
        _session: MarketSession,
        flow: FlowState,
        shock_active: bool,
    ) -> f64 {
        const ALT_SL_VOL_BUFFER_ATR: f64 = 0.18;
        let mut factor = 1.0;
        if shock_active {
            factor += ALT_SL_VOL_BUFFER_ATR;
        }
        if matches!(flow, FlowState::Low) {
            factor += 0.08;
        }
        f_clamp(factor, 1.0, 1.85)
    }

    /// Pine V62 line 389:
    /// ```text
    /// altTPFactor = f_clamp(
    ///     (altWildVol  ? altTPWildCompress : 1.0)     // 0.74 if vol shock
    ///   * (altThinFlow ? altTPThinCompress : 1.0)     // 0.66 if thin flow
    ///   * (altCleanImpulse and flowState == "TINGGI"
    ///                        ? 1.08 : 1.0),
    ///     0.38, 1.10)
    /// ```
    /// Altcoin compresses TPs harder than Gold (0.74 / 0.66 vs 0.88 / 0.82)
    /// because alt liquidity dries up faster on shock bars. No session
    /// component — Altcoin V62 treats the session via `altEWFactor` only.
    fn tp_compression_factor(
        &self,
        _session: MarketSession,
        flow: FlowState,
        shock_active: bool,
    ) -> f64 {
        const ALT_TP_WILD_COMPRESS: f64 = 0.74;
        const ALT_TP_THIN_COMPRESS: f64 = 0.66;
        let alt_wild_vol = shock_active;
        let alt_thin_flow = matches!(flow, FlowState::Low);
        let vol_mult = if alt_wild_vol { ALT_TP_WILD_COMPRESS } else { 1.0 };
        let flow_mult = if alt_thin_flow { ALT_TP_THIN_COMPRESS } else { 1.0 };
        f_clamp(vol_mult * flow_mult, 0.38, 1.10)
    }

    /// Pine V62 lines 708-718 — Altcoin direction adapter. Same LTF-edge
    /// weighting shape as Gold V63 but with `altLTFWeight = 1.25` (vs
    /// Gold's 0.88) and 0.10/0.08 multiplier pair (vs Gold's symmetric
    /// 0.07). No proxy contribution — alt majors don't carry an XAUUSD
    /// equivalent.
    fn score_adjustments(
        &self,
        direction: SignalDirection,
        snapshot: &IndicatorSnapshot<'_>,
    ) -> f64 {
        let mut adj = 0.0;
        let consensus = &snapshot.consensus;
        let m15_bias = snapshot
            .mtf
            .get(&Timeframe::M15)
            .map(|s| s.bias)
            .unwrap_or(SignalDirection::Wait);
        let alt_ltf_long = consensus.long_score > consensus.short_score
            && m15_bias == SignalDirection::Long;
        let alt_ltf_short = consensus.short_score > consensus.long_score
            && m15_bias == SignalDirection::Short;
        match direction {
            SignalDirection::Long => {
                if alt_ltf_long {
                    adj += consensus.long_score * 0.10 * ALT_LTF_WEIGHT;
                }
                if alt_ltf_short {
                    adj -= consensus.short_score * 0.08 * ALT_LTF_WEIGHT;
                }
            }
            SignalDirection::Short => {
                if alt_ltf_short {
                    adj += consensus.short_score * 0.10 * ALT_LTF_WEIGHT;
                }
                if alt_ltf_long {
                    adj -= consensus.long_score * 0.08 * ALT_LTF_WEIGHT;
                }
            }
            SignalDirection::Wait => return 0.0,
        }

        // altLongReversalOK / altShortReversalOK +10.0 (Pine lines 713-714).
        let h1_bias = snapshot
            .mtf
            .get(&Timeframe::H1)
            .map(|s| s.bias)
            .unwrap_or(SignalDirection::Wait);
        let reversal_ok = match direction {
            SignalDirection::Long => {
                alt_ltf_long
                    && m15_bias == SignalDirection::Long
                    && h1_bias == SignalDirection::Long
            }
            SignalDirection::Short => {
                alt_ltf_short
                    && m15_bias == SignalDirection::Short
                    && h1_bias == SignalDirection::Short
            }
            SignalDirection::Wait => false,
        };
        if reversal_ok {
            adj += 10.0;
        }

        // Macro-conflict relax (+10 * altHTFRelax). Pine lines 717-718.
        let bias_w1 = snapshot
            .mtf
            .get(&Timeframe::W1)
            .map(|s| s.bias)
            .unwrap_or(SignalDirection::Wait);
        let bias_mn = snapshot
            .mtf
            .get(&Timeframe::Mn1)
            .map(|s| s.bias)
            .unwrap_or(SignalDirection::Wait);
        let macro_conflict = match direction {
            SignalDirection::Long => {
                bias_w1 == SignalDirection::Short || bias_mn == SignalDirection::Short
            }
            SignalDirection::Short => {
                bias_w1 == SignalDirection::Long || bias_mn == SignalDirection::Long
            }
            SignalDirection::Wait => false,
        };
        if macro_conflict && reversal_ok {
            adj += 10.0 * ALT_HTF_RELAX;
        }

        adj
    }
}
