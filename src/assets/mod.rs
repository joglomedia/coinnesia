pub mod altcoin;
pub mod btc;
pub mod forex;
pub mod gold;
pub mod stocks_idx;

use crate::{
    config::{profiles, AppConfig},
    strategy::{
        plan_context::FlowState,
        session::MarketSession,
        signals::{evaluate_direction, DirectionEvaluation, IndicatorSnapshot},
        SignalDirection,
    },
    AssetClass,
};

pub fn philosophy(asset_class: AssetClass) -> &'static str {
    profiles::default_profiles()
        .get(&asset_class)
        .map(|profile| profile.philosophy)
        .unwrap_or("unknown")
}

/// Per-asset-class evaluator dispatch. Each impl can:
/// 1. Adjust the additive `DirectionEvaluation` produced by the shared
///    six-layer scorer (default: forward unchanged).
/// 2. Apply a hard gate that overrides `passes` and tags the reason with a
///    Pine-equivalent string (e.g. `gold_proxy_block:short`,
///    `forex_htf_counter_block:long`, `idx_rvol_below_min:0.85`). Hard gates
///    are how Pine's V1 / V5 / V58 scripts reject otherwise-passing setups.
pub(crate) trait AssetEvaluator {
    fn asset_class(&self) -> AssetClass;

    /// Run the shared additive six-layer scorer. The default forwards to
    /// `signals::evaluate_direction`; classes only override when they need to
    /// tweak the score after the fact (none currently do).
    fn evaluate(
        &self,
        direction: SignalDirection,
        snapshot: &IndicatorSnapshot<'_>,
        config: &AppConfig,
    ) -> DirectionEvaluation {
        evaluate_direction(direction, self.asset_class(), snapshot, config)
    }

    /// Returns `Some(reason)` when an asset-class-specific hard gate must
    /// reject the candidate even if the additive score passed. The default
    /// applies no extra gate.
    fn extra_gate(
        &self,
        _direction: SignalDirection,
        _snapshot: &IndicatorSnapshot<'_>,
        _config: &AppConfig,
    ) -> Option<String> {
        None
    }

    /// Sub-phase 1.7.16 Gap C — Pine `altEWFactor` (Gold V1 line 400, Altcoin
    /// V62 line 388). Multiplies the EW2 / EW3 / Deep-add spacing to shrink
    /// the entry ladder under thin-flow or off-peak-session conditions. The
    /// default returns `1.0` (no compression — matches BTC V61.9, Forex V58,
    /// IDX V5, which leave the EW spacing un-multiplied). Asset classes with
    /// a Pine `altEWFactor` override this method.
    fn ew_compression_factor(
        &self,
        _session: MarketSession,
        _flow: FlowState,
        _shock_active: bool,
    ) -> f64 {
        1.0
    }

    /// Sub-phase 1.7.16 Gap G — Pine `altSLFactor` (Gold V1 line 402, Altcoin
    /// V62 line 390). Multiplies the absolute `maxSLDistanceATR` cap so the
    /// SL engine can accept a wider stop under chaotic conditions. Returns
    /// `1.0` by default (no extension). Gold / Altcoin override with their
    /// V1 / V62 formulas; clamped to `[1.0, 1.55]` and `[1.0, 1.85]`
    /// respectively in those impls.
    fn sl_extension_factor(
        &self,
        _session: MarketSession,
        _flow: FlowState,
        _shock_active: bool,
    ) -> f64 {
        1.0
    }

    /// Sub-phase 1.7.16 Gap H — Pine `altTPFactor` (Gold V1 line 401, Altcoin
    /// V62 line 389). Per-asset multiplier on the TP raw distance
    /// (`base ± atr * tpNATR * trendTPFactor * sessTPFactor * altTPFactor`).
    /// Returns `1.0` by default (no compression). Gold / Altcoin override;
    /// clamped to `[0.55, 1.08]` and `[0.38, 1.10]` respectively.
    fn tp_compression_factor(
        &self,
        _session: MarketSession,
        _flow: FlowState,
        _shock_active: bool,
    ) -> f64 {
        1.0
    }

    /// Sub-phase 1.7.17 commit 4 — Pine V63 Gold direction adapter (Gold V1
    /// lines 731-745, Altcoin V62 analogous block). Returns a single
    /// post-clamp score contribution that the accumulator adds verbatim.
    /// The default returns `0.0` (no adjustment, matches BTC V61.9 /
    /// Forex V58 / IDX V5 which omit this block). Gold and Altcoin
    /// override with their respective V63 / V62 adapters covering:
    ///   - LTF edge weighting (`consensusScore * 0.07 * altLTFWeight`)
    ///   - Directional proxy bonus (`+goldProxyWeight` / `-goldProxyWeight`)
    ///   - Reversal-OK +8 bonus
    ///   - Macro-conflict relax (`+8 * altHTFRelax`)
    ///   - Opposing-proxy −8 penalty
    ///
    /// Token-specific Pine signals (`altClean`, `altChaos`,
    /// `altFakeImpulse`, `liqBullReclaim`) are approximated via the
    /// available `flow`/`shock_active` proxies — full refinement tracked
    /// under sub-phase 1.7.16 Gap F.
    fn score_adjustments(
        &self,
        _direction: SignalDirection,
        _snapshot: &crate::strategy::signals::IndicatorSnapshot<'_>,
    ) -> f64 {
        0.0
    }
}

/// Look up the `AssetEvaluator` for the given class. Returns a boxed trait
/// object so the call site can be class-agnostic.
pub(crate) fn evaluator_for(asset_class: AssetClass) -> Box<dyn AssetEvaluator> {
    match asset_class {
        AssetClass::Btc => Box::new(btc::BtcEvaluator),
        AssetClass::Altcoin => Box::new(altcoin::AltcoinEvaluator),
        AssetClass::Gold => Box::new(gold::GoldEvaluator),
        AssetClass::Forex => Box::new(forex::ForexEvaluator),
        AssetClass::StocksIdx => Box::new(stocks_idx::StocksIdxEvaluator),
        AssetClass::StocksUs => Box::new(stocks_idx::StocksIdxEvaluator),
    }
}
