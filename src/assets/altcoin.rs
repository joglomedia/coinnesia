use crate::{
    assets::AssetEvaluator,
    config::AppConfig,
    strategy::{signals::IndicatorSnapshot, SignalDirection},
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
}
