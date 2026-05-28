use crate::{
    assets::AssetEvaluator,
    config::AppConfig,
    strategy::{
        session::{classify_wib, MarketSession},
        signals::IndicatorSnapshot,
        SignalDirection,
    },
    AssetClass,
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
}
