use crate::{
    assets::AssetEvaluator,
    config::AppConfig,
    strategy::{signals::IndicatorSnapshot, SignalDirection},
    AssetClass,
};

/// Pine V5 IDX philosophy: volume gate first. The script blocks trade
/// emission when relative volume falls below `rvolMin` (default 1.20) —
/// thin-tape sessions are too dangerous for the IDX universe.
///
/// Acceptance for 1.7.8: IDX setups Wait when RVOL < 1.20 or value-traded
/// below threshold.
pub const PHILOSOPHY: &str = "volume_guard_first";

pub struct StocksIdxEvaluator;

impl AssetEvaluator for StocksIdxEvaluator {
    fn asset_class(&self) -> AssetClass {
        AssetClass::StocksIdx
    }

    fn extra_gate(
        &self,
        direction: SignalDirection,
        snapshot: &IndicatorSnapshot<'_>,
        config: &AppConfig,
    ) -> Option<String> {
        // RVOL hard gate. Pine: `if rvol < rvolMin → noTrade`. When the rvol
        // reading is still pending (insufficient bars) we abstain rather than
        // false-block — the additive layers will already report their own
        // `pending` reasons.
        if snapshot.rvol.rvol.ready
            && snapshot.rvol.rvol.value < config.indicators.rvol_min
        {
            return Some(format!(
                "idx_rvol_below_min:{:.2}<{:.2}",
                snapshot.rvol.rvol.value, config.indicators.rvol_min
            ));
        }

        // Pine `avgValueOK = valueRatio >= 1.0`. We treat value ratio < 1.0 as
        // a hard reject only when ALSO RVOL is exactly at parity — combined
        // failure indicates a quiet tape that cannot support the proposed
        // entry. Single-axis weakness flows through the soft layer.
        if snapshot.rvol.value_ratio.ready
            && snapshot.rvol.value_ratio.value < 0.85
            && snapshot.rvol.rvol.ready
            && snapshot.rvol.rvol.value < config.indicators.rvol_min
        {
            return Some(format!(
                "idx_value_traded_below_threshold:{:.2}",
                snapshot.rvol.value_ratio.value
            ));
        }

        // V5 IDX IHSG benchmark hard block: when the daily IHSG bias is
        // actively against `direction`, the trade fights the broader index.
        // Pine treats this as a no-go for IDX constituents.
        match (direction, snapshot.ihsg_bias) {
            (SignalDirection::Long, SignalDirection::Short) => {
                Some("idx_ihsg_block:ihsg_short_vs_long".to_owned())
            }
            (SignalDirection::Short, SignalDirection::Long) => {
                Some("idx_ihsg_block:ihsg_long_vs_short".to_owned())
            }
            _ => None,
        }
    }
}
