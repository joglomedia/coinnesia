use crate::{assets::AssetEvaluator, AssetClass};

/// Pine V61.9 BTC philosophy: structure-first ordering. The shared six-layer
/// scorer already encodes BOS/CHOCH structure as the highest-weight gate, so
/// the BTC evaluator is the trait default — no extra hard gate beyond the
/// global trap-guard, regime, and consensus filters in `signals.rs`.
pub const PHILOSOPHY: &str = "structure_first";

pub struct BtcEvaluator;

impl AssetEvaluator for BtcEvaluator {
    fn asset_class(&self) -> AssetClass {
        AssetClass::Btc
    }
}
