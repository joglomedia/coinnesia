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
}
