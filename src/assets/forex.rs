use crate::{
    assets::AssetEvaluator,
    config::AppConfig,
    strategy::{signals::IndicatorSnapshot, SignalDirection},
    AssetClass,
};

/// Pine V58 Forex philosophy: session × R:R + HTF bias gate. The daily +
/// H4 aggregate bias (Pine `aggregate_htf_bias`) rejects counter-trend
/// setups when both higher timeframes agree against `direction` — Pine's
/// `blockCounterHTF` boolean.
///
/// Acceptance for 1.7.8: Forex setups Wait when HTF bias opposes primary.
pub const PHILOSOPHY: &str = "session_rr_first";

pub struct ForexEvaluator;

impl AssetEvaluator for ForexEvaluator {
    fn asset_class(&self) -> AssetClass {
        AssetClass::Forex
    }

    fn extra_gate(
        &self,
        direction: SignalDirection,
        snapshot: &IndicatorSnapshot<'_>,
        _config: &AppConfig,
    ) -> Option<String> {
        let Some(htf) = snapshot.htf_bias_aggregate else {
            return None;
        };
        match direction {
            SignalDirection::Long if htf.block_long => Some(format!(
                "forex_htf_counter_block:long short_score={}/{}",
                htf.short_score, htf.max_score
            )),
            SignalDirection::Short if htf.block_short => Some(format!(
                "forex_htf_counter_block:short long_score={}/{}",
                htf.long_score, htf.max_score
            )),
            _ => None,
        }
    }
}
