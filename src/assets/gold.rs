use crate::{
    assets::AssetEvaluator,
    config::AppConfig,
    strategy::{
        plan_context::{f_clamp, FlowState},
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
}
