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
