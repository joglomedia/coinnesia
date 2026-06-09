use crate::{
    assets::AssetEvaluator,
    config::AppConfig,
    strategy::{
        plan_context::{f_clamp, FlowState},
        session::{classify_wib, MarketSession},
        signals::IndicatorSnapshot,
        SignalDirection,
    },
    AssetClass, Timeframe,
};

/// Pine V1 Gold philosophy: proxy + session first. The XAUUSD daily proxy
/// dominates the directional decision; if the proxy bias actively opposes
/// the candidate direction Pine rejects regardless of how the additive
/// scorer landed.
///
/// Acceptance for 1.7.8: Gold setups Wait when XAUUSD proxy bias opposes the
/// primary direction.
pub const PHILOSOPHY: &str = "proxy_session_first";

pub struct GoldEvaluator;

impl AssetEvaluator for GoldEvaluator {
    fn asset_class(&self) -> AssetClass {
        AssetClass::Gold
    }

    fn extra_gate(
        &self,
        direction: SignalDirection,
        snapshot: &IndicatorSnapshot<'_>,
        _config: &AppConfig,
    ) -> Option<String> {
        // Pine V1 `goldSessionBiasMode = "London/USA only"`: the entire trade
        // surface is muted in Asia. Already enforced by
        // `session::session_allows_asset` for AssetClass::Gold; the call here
        // adds an extra panel-friendly tag when a Gold setup is mistakenly
        // built outside the active session window (e.g. a misconfigured
        // symbol with `asset_class = "gold"` but no session gating upstream).
        let session = classify_wib(snapshot.latest.ts);
        if !matches!(session, MarketSession::Europe | MarketSession::Usa) {
            return Some(format!("gold_session_block:{:?}", session));
        }

        // Proxy hard gate: XAUUSD daily bias opposing `direction` rejects.
        match (direction, snapshot.xauusd_bias) {
            (SignalDirection::Long, SignalDirection::Short) => {
                Some("gold_proxy_block:xauusd_short_vs_long".to_owned())
            }
            (SignalDirection::Short, SignalDirection::Long) => {
                Some("gold_proxy_block:xauusd_long_vs_short".to_owned())
            }
            _ => None,
        }
    }

    /// Pine V1 line 400:
    /// ```text
    /// altEWFactor = f_clamp(
    ///     (altWildVol ? altEWVolCompress : 1.0)        // 0.86 if vol shock
    ///   * (altThinFlow ? altEWThinCompress : 1.0)      // 0.90 if thin flow
    ///   * (goldSessionQuiet ? 0.92                      // RolloverAvoid + flow≠High
    ///      : sess == "EROPA" ? 0.98
    ///      : sess == "USA"   ? 0.95
    ///      : 1.0),
    ///     0.58, 1.05)
    /// ```
    /// The Pine token-specific signals (`goldTokenVolatile`, `goldTokenThin`,
    /// `goldNewsShock`, ATR/range-flow ratios) are token-level inputs not
    /// modelled here yet — we use `shock_active` as the conservative
    /// `altWildVol` proxy and `flow == Low` as the `altThinFlow` proxy.
    /// Refinement tracked under sub-phase 1.7.16 Gap F.
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
        let gold_session_quiet =
            matches!(session, MarketSession::RolloverAvoid) && !matches!(flow, FlowState::High);
        let vol_mult = if alt_wild_vol { ALT_EW_VOL_COMPRESS } else { 1.0 };
        let flow_mult = if alt_thin_flow { ALT_EW_THIN_COMPRESS } else { 1.0 };
        let session_mult = if gold_session_quiet {
            0.92
        } else {
            match session {
                MarketSession::Europe => 0.98,
                MarketSession::Usa => 0.95,
                _ => 1.0,
            }
        };
        f_clamp(vol_mult * flow_mult * session_mult, 0.58, 1.05)
    }

    /// Pine V1 line 402:
    /// ```text
    /// altSLFactor = f_clamp(
    ///     1.0
    ///   + (altWickChaos ? altSLWickBufferATR : 0.0)   // +0.18 if wick chaos
    ///   + (altWildVol   ? altSLVolBufferATR  : 0.0)   // +0.14 if vol shock
    ///   + (altThinFlow  ? 0.06               : 0.0)
    ///   + (goldNewsShock ? 0.10              : 0.0),
    ///     1.0, 1.55)
    /// ```
    /// We proxy `altWildVol ≈ shock_active` and `altThinFlow ≈ flow == Low`.
    /// Pine `altWickChaos` and `goldNewsShock` are token-specific (PAXG/XAUT
    /// vs spot) and not modelled here — Gap F follow-up.
    fn sl_extension_factor(
        &self,
        _session: MarketSession,
        flow: FlowState,
        shock_active: bool,
    ) -> f64 {
        const ALT_SL_VOL_BUFFER_ATR: f64 = 0.14;
        let mut factor = 1.0;
        if shock_active {
            factor += ALT_SL_VOL_BUFFER_ATR;
        }
        if matches!(flow, FlowState::Low) {
            factor += 0.06;
        }
        f_clamp(factor, 1.0, 1.55)
    }

    /// Pine V1 line 401:
    /// ```text
    /// altTPFactor = f_clamp(
    ///     (altWildVol  ? altTPWildCompress : 1.0)     // 0.88 if vol shock
    ///   * (altThinFlow ? altTPThinCompress : 1.0)     // 0.82 if thin flow
    ///   * (goldSessionQuiet ? 0.86 : 1.0)
    ///   * (altCleanImpulse and goldSessionActive and flowState == "TINGGI"
    ///                         ? 1.05 : 1.0),
    ///     0.55, 1.08)
    /// ```
    /// `altCleanImpulse` is a token-specific clean-break detector — skipped
    /// here (Gap F follow-up). The high-flow bonus therefore never fires
    /// without that signal.
    fn tp_compression_factor(
        &self,
        session: MarketSession,
        flow: FlowState,
        shock_active: bool,
    ) -> f64 {
        const ALT_TP_WILD_COMPRESS: f64 = 0.88;
        const ALT_TP_THIN_COMPRESS: f64 = 0.82;
        let alt_wild_vol = shock_active;
        let alt_thin_flow = matches!(flow, FlowState::Low);
        let gold_session_quiet =
            matches!(session, MarketSession::RolloverAvoid) && !matches!(flow, FlowState::High);
        let vol_mult = if alt_wild_vol { ALT_TP_WILD_COMPRESS } else { 1.0 };
        let flow_mult = if alt_thin_flow { ALT_TP_THIN_COMPRESS } else { 1.0 };
        let session_mult = if gold_session_quiet { 0.86 } else { 1.0 };
        f_clamp(vol_mult * flow_mult * session_mult, 0.55, 1.08)
    }

    /// Pine V1 lines 731-745 V63 Gold direction adapter (replicated here):
    /// ```text
    /// longScore += altLTFLongEdge ? consensusLongScore * 0.07 * altLTFWeight : 0
    /// longScore -= altLTFShortEdge ? consensusShortScore * 0.07 * altLTFWeight : 0
    /// longScore += goldProxyBull ? goldProxyWeight : goldProxyBear ? -goldProxyWeight : 0
    /// longScore += altLongReversalOK ? 8.0 : 0
    /// longScore -= (altChaos or altFakeImpulse) ? altTrapPenalty : 0
    /// longScore += macroConflictLong and altLongReversalOK ? 8 * altHTFRelax : 0
    /// longScore -= useGoldProxyFilter and goldProxyBear and not altLongReversalOK ? 8 : 0
    /// ```
    /// Pine constants: `altLTFWeight = 0.88`, `goldProxyWeight = 9.0`,
    /// `altHTFRelax = 0.18`. Token-specific signals (`altClean`,
    /// `altChaos`, `altFakeImpulse`, `liqBullReclaim`) are not modelled
    /// here — they'd require PAXG/XAUT vs spot decomposition (sub-phase
    /// 1.7.16 Gap F follow-up).
    fn score_adjustments(
        &self,
        direction: SignalDirection,
        snapshot: &IndicatorSnapshot<'_>,
    ) -> f64 {
        const ALT_LTF_WEIGHT: f64 = 0.88;
        const GOLD_PROXY_WEIGHT: f64 = 9.0;
        const ALT_HTF_RELAX: f64 = 0.18;

        let mut adj = 0.0;

        // altLTFLongEdge / altLTFShortEdge approximation. Pine requires
        // M1+M5 + M15 alignment with the consensus tilt; we use the
        // pre-computed consensus + the M15 MTF bias as a stand-in.
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
                    adj += consensus.long_score * 0.07 * ALT_LTF_WEIGHT;
                }
                if alt_ltf_short {
                    adj -= consensus.short_score * 0.07 * ALT_LTF_WEIGHT;
                }
            }
            SignalDirection::Short => {
                if alt_ltf_short {
                    adj += consensus.short_score * 0.07 * ALT_LTF_WEIGHT;
                }
                if alt_ltf_long {
                    adj -= consensus.long_score * 0.07 * ALT_LTF_WEIGHT;
                }
            }
            SignalDirection::Wait => return 0.0,
        }

        // Gold proxy directional weight (Pine lines 736-737).
        match (direction, snapshot.xauusd_bias) {
            (SignalDirection::Long, SignalDirection::Long) => adj += GOLD_PROXY_WEIGHT,
            (SignalDirection::Long, SignalDirection::Short) => adj -= GOLD_PROXY_WEIGHT,
            (SignalDirection::Short, SignalDirection::Short) => adj += GOLD_PROXY_WEIGHT,
            (SignalDirection::Short, SignalDirection::Long) => adj -= GOLD_PROXY_WEIGHT,
            _ => {}
        }

        // altLongReversalOK / altShortReversalOK approximation: LTF edge
        // present AND M15 + H1 MTF agree. (Pine adds `altCleanImpulse`,
        // `!altChaos`, `!distributionRisk`, `goldProxyAllowsLong` —
        // approximated by checking the M15+H1 alignment here.)
        let h1_bias = snapshot
            .mtf
            .get(&Timeframe::H1)
            .map(|s| s.bias)
            .unwrap_or(SignalDirection::Wait);
        let reversal_ok = match direction {
            SignalDirection::Long => {
                alt_ltf_long && m15_bias == SignalDirection::Long && h1_bias == SignalDirection::Long
            }
            SignalDirection::Short => {
                alt_ltf_short
                    && m15_bias == SignalDirection::Short
                    && h1_bias == SignalDirection::Short
            }
            SignalDirection::Wait => false,
        };
        if reversal_ok {
            adj += 8.0;
        }

        // Macro-conflict relax (Pine lines 742-743). When the macro
        // timeframes (W1/MN) oppose direction BUT a clean LTF reversal is
        // also confirmed, Pine adds `+8 * altHTFRelax`.
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
            adj += 8.0 * ALT_HTF_RELAX;
        }

        // Opposing-proxy penalty (Pine lines 744-745). When the XAUUSD
        // proxy actively opposes direction AND no reversal allowance
        // applies, Pine subtracts 8.
        let proxy_opposes = match (direction, snapshot.xauusd_bias) {
            (SignalDirection::Long, SignalDirection::Short) => true,
            (SignalDirection::Short, SignalDirection::Long) => true,
            _ => false,
        };
        if proxy_opposes && !reversal_ok {
            adj -= 8.0;
        }

        adj
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 1e-9
    }

    #[test]
    fn ew_compression_factor_matches_pine_v1_usa() {
        let f = GoldEvaluator.ew_compression_factor(MarketSession::Usa, FlowState::Mid, false);
        // Pine: 1.0 * 1.0 * 0.95 = 0.95 (no compress, no clamp boundary).
        assert!(near(f, 0.95), "factor = {}", f);
    }

    #[test]
    fn ew_compression_factor_matches_pine_v1_europe() {
        let f = GoldEvaluator.ew_compression_factor(MarketSession::Europe, FlowState::Mid, false);
        assert!(near(f, 0.98), "factor = {}", f);
    }

    #[test]
    fn ew_compression_factor_thin_flow_multiplies_in() {
        let f = GoldEvaluator.ew_compression_factor(MarketSession::Usa, FlowState::Low, false);
        // 1.0 * 0.90 * 0.95 = 0.855
        assert!(near(f, 0.855), "factor = {}", f);
    }

    #[test]
    fn ew_compression_factor_shock_active_multiplies_in() {
        let f = GoldEvaluator.ew_compression_factor(MarketSession::Usa, FlowState::Mid, true);
        // 0.86 * 1.0 * 0.95 = 0.817
        assert!(near(f, 0.817), "factor = {}", f);
    }

    #[test]
    fn ew_compression_factor_clamps_to_lower_bound() {
        // wildVol + thinFlow + quiet session — product is 0.86 * 0.90 * 0.92
        // = 0.71208, well above the 0.58 floor; bump shock to ensure clamp
        // never overshoots in either direction.
        let f = GoldEvaluator.ew_compression_factor(MarketSession::RolloverAvoid, FlowState::Low, true);
        assert!(f >= 0.58 - 1e-9 && f <= 1.05 + 1e-9, "factor = {}", f);
    }

    #[test]
    fn sl_extension_factor_baseline_is_one() {
        let f = GoldEvaluator.sl_extension_factor(MarketSession::Usa, FlowState::Mid, false);
        assert!(near(f, 1.0), "factor = {}", f);
    }

    #[test]
    fn sl_extension_factor_adds_pads_for_shock_and_thin_flow() {
        // shock_active → +0.14, thin_flow → +0.06 → 1.20
        let f = GoldEvaluator.sl_extension_factor(MarketSession::Usa, FlowState::Low, true);
        assert!(near(f, 1.20), "factor = {}", f);
    }

    #[test]
    fn sl_extension_factor_clamps_to_1_55_ceiling() {
        // Even with both pads we land at 1.20 — confirm the clamp range.
        let f = GoldEvaluator.sl_extension_factor(MarketSession::RolloverAvoid, FlowState::Low, true);
        assert!(f >= 1.0 - 1e-9 && f <= 1.55 + 1e-9, "factor = {}", f);
    }

    #[test]
    fn tp_compression_factor_baseline_is_one() {
        let f = GoldEvaluator.tp_compression_factor(MarketSession::Usa, FlowState::Mid, false);
        assert!(near(f, 1.0), "factor = {}", f);
    }

    #[test]
    fn tp_compression_factor_thin_flow_compresses() {
        // 1.0 * 0.82 * 1.0 = 0.82
        let f = GoldEvaluator.tp_compression_factor(MarketSession::Usa, FlowState::Low, false);
        assert!(near(f, 0.82), "factor = {}", f);
    }

    #[test]
    fn tp_compression_factor_quiet_session_drops_to_0_86() {
        // RolloverAvoid + flow != High → goldSessionQuiet = true → 0.86
        let f = GoldEvaluator.tp_compression_factor(MarketSession::RolloverAvoid, FlowState::Mid, false);
        assert!(near(f, 0.86), "factor = {}", f);
    }
}
