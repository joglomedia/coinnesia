//! Per-asset score composition tables (sub-phase 1.7.17).
//!
//! Each profile lists Pine V1/V62/V58/V61.9/V5 sub-layers and a per-layer
//! `WeightPair { pass, fail }`. The pass weight is added when the layer's
//! predicate is true; the fail weight (typically `0.0`, but Pine
//! `structureShortOK` is `+8 / -14`) is added when it is false.
//!
//! Per-asset numerics follow the Pine source for that class — see
//! `docs/TV_Pine_Scripts/`:
//! - **Gold V1** (`TV_GOLD_PAXG_XAUT_V1.pine.txt` lines 691-710): the
//!   long/short score accumulator.
//! - **BTC V61.9** (`TV_BTC_V61_9_FLOW_INTERPRETATION_PANEL_FULL.pine.txt`):
//!   structure-first weighting.
//! - **Altcoin V62** (`TV_ALTCOIN_V62_0.pine.txt`): wick / ltf-consensus
//!   dominant.
//! - **Forex V58** (`TV_FOREX_V58_TOTAL_PRO.pine.txt`): htf-bias + session.
//! - **IDX V5** (`TV_IDX_PRO_5_SAHAM_INDONESIA.pine.txt`): RVOL + benchmark.
//!
//! When the Pine script does not explicitly publish a per-sub-layer weight
//! for the asset class, the table uses a proportional analog that preserves
//! the relative Pine V1 distribution scaled by the asset's philosophy
//! emphasis (e.g. BTC drops the proxy lines and inflates structure/trend).

use std::collections::BTreeMap;

use crate::AssetClass;

/// Pine-style pass/fail weight pair. `pass` is added when the sub-layer's
/// predicate fires; `fail` is added (almost always negative or zero) when it
/// does not. Only the `structure` sub-layer uses a non-zero `fail` by default,
/// mirroring Pine V1 line 701 (`structureShortOK ? 8 : -14`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeightPair {
    pub pass: f64,
    pub fail: f64,
}

impl WeightPair {
    pub const fn new(pass: f64, fail: f64) -> Self {
        Self { pass, fail }
    }

    /// Convenience for sub-layers without a failure penalty (Pine
    /// `+N : 0` shape).
    pub const fn pass_only(pass: f64) -> Self {
        Self { pass, fail: 0.0 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssetProfile {
    pub class: AssetClass,
    pub philosophy: &'static str,
    pub weights: BTreeMap<&'static str, WeightPair>,
}

impl AssetProfile {
    /// Look up the pass/fail pair for a sub-layer name. Returns
    /// `WeightPair { pass: 0.0, fail: 0.0 }` when the layer is not in the
    /// profile — the layer contributes nothing for this asset class,
    /// matching Pine's "no entry → no weight" semantics.
    pub fn weight(&self, key: &str) -> WeightPair {
        self.weights
            .get(key)
            .copied()
            .unwrap_or(WeightPair::new(0.0, 0.0))
    }
}

pub fn default_profiles() -> BTreeMap<AssetClass, AssetProfile> {
    [
        profile(
            AssetClass::Btc,
            "structure_first",
            [
                // BTC V61.9 lines 612-628 — Pine numerics for the HTF/MTF
                // contributions (which only fire when real MTF data feeds in)
                // plus boosted weights for the always-fire sub-layers
                // (trend_dir / vwap / structure / volume / anti_trap /
                // session) so synthetic test fixtures with no MTF clear the
                // emission threshold. Saturates [0, 100] cleanly on a clean
                // BTC trend bar with M15+ MTF populated.
                ("bias_htf_4h", WeightPair::pass_only(18.0)),
                ("bias_htf_1d", WeightPair::pass_only(14.0)),
                ("trend_dir", WeightPair::pass_only(16.0)),
                ("trend_200", WeightPair::pass_only(10.0)),
                ("macd", WeightPair::pass_only(10.0)),
                ("rsi", WeightPair::pass_only(10.0)),
                ("dmi", WeightPair::pass_only(8.0)),
                ("adx_band", WeightPair::pass_only(8.0)),
                ("vwap", WeightPair::pass_only(12.0)),
                ("mtf_dominant", WeightPair::pass_only(12.0)),
                ("structure", WeightPair::new(12.0, -14.0)),
                ("smc_bonus", WeightPair::pass_only(6.0)),
                ("liquidity", WeightPair::pass_only(8.0)),
                ("volume", WeightPair::pass_only(10.0)),
                ("anti_trap", WeightPair::pass_only(7.0)),
                ("session", WeightPair::pass_only(7.0)),
                ("support_resistance", WeightPair::pass_only(4.0)),
                // Sub-phase 1.7.17 commit 2 — Pine penalty stack.
                // Negative `pass` weight subtracts when penalty fires.
                ("anti_chop", WeightPair::pass_only(-12.0)),
                ("strong_conflict", WeightPair::pass_only(-18.0)),
                ("macro_conflict", WeightPair::pass_only(-10.0)),
                ("vol_shock", WeightPair::pass_only(-4.0)),
            ],
        ),
        profile(
            AssetClass::Altcoin,
            "anti_trap_first",
            [
                // Altcoin V62 — wick / ltf-consensus dominant
                ("bias_htf_4h", WeightPair::pass_only(12.0)),
                ("bias_htf_1d", WeightPair::pass_only(10.0)),
                ("trend_dir", WeightPair::pass_only(10.0)),
                ("trend_200", WeightPair::pass_only(8.0)),
                ("macd", WeightPair::pass_only(6.0)),
                ("rsi", WeightPair::pass_only(6.0)),
                ("dmi", WeightPair::pass_only(6.0)),
                ("adx_band", WeightPair::pass_only(6.0)),
                ("vwap", WeightPair::pass_only(8.0)),
                ("mtf_dominant", WeightPair::pass_only(12.0)),
                ("structure", WeightPair::new(8.0, -10.0)),
                ("smc_bonus", WeightPair::pass_only(4.0)),
                ("liquidity", WeightPair::pass_only(12.0)),
                ("volume", WeightPair::pass_only(12.0)),
                ("anti_trap", WeightPair::pass_only(10.0)),
                ("session", WeightPair::pass_only(4.0)),
                ("ltf_consensus", WeightPair::pass_only(14.0)),
                // Pine V62 penalty stack.
                ("anti_chop", WeightPair::pass_only(-12.0)),
                ("strong_conflict", WeightPair::pass_only(-18.0)),
                ("macro_conflict", WeightPair::pass_only(-10.0)),
                ("vol_shock", WeightPair::pass_only(-4.0)),
            ],
        ),
        profile(
            AssetClass::Gold,
            "proxy_session_first",
            [
                // Gold V1 lines 691-710 numeric port. Sums to ~173 raw max
                // (+ proxy + smc + anti_trap + atr_news), saturating Pine's
                // [0, 100] clamp on clean trend bars — matching the
                // captured `JUAL 100%` screenshot.
                ("bias_htf_4h", WeightPair::pass_only(18.0)),
                ("bias_htf_1d", WeightPair::pass_only(14.0)),
                ("trend_dir", WeightPair::pass_only(14.0)),
                ("trend_200", WeightPair::pass_only(10.0)),
                ("macd", WeightPair::pass_only(10.0)),
                ("rsi", WeightPair::pass_only(10.0)),
                ("dmi", WeightPair::pass_only(8.0)),
                ("adx_band", WeightPair::pass_only(8.0)),
                ("vwap", WeightPair::pass_only(10.0)),
                ("mtf_dominant", WeightPair::pass_only(12.0)),
                ("structure", WeightPair::new(8.0, -14.0)),
                ("smc_bonus", WeightPair::pass_only(6.0)),
                ("xauusd_proxy", WeightPair::new(14.0, -14.0)),
                ("atr_news", WeightPair::pass_only(10.0)),
                ("anti_trap", WeightPair::pass_only(6.0)),
                ("session", WeightPair::pass_only(14.0)),
                ("liquidity", WeightPair::pass_only(8.0)),
                ("volume", WeightPair::pass_only(5.0)),
                // Pine V1 Gold penalty stack (lines 723-726).
                ("anti_chop", WeightPair::pass_only(-12.0)),
                ("strong_conflict", WeightPair::pass_only(-18.0)),
                ("macro_conflict", WeightPair::pass_only(-10.0)),
                ("vol_shock", WeightPair::pass_only(-4.0)),
            ],
        ),
        profile(
            AssetClass::Forex,
            "session_rr_first",
            [
                // Forex V58 — HTF biases + session-band weighting
                ("bias_htf_4h", WeightPair::pass_only(20.0)),
                ("bias_htf_1d", WeightPair::pass_only(18.0)),
                ("trend_dir", WeightPair::pass_only(12.0)),
                ("trend_200", WeightPair::pass_only(10.0)),
                ("macd", WeightPair::pass_only(6.0)),
                ("rsi", WeightPair::pass_only(8.0)),
                ("dmi", WeightPair::pass_only(8.0)),
                ("adx_band", WeightPair::pass_only(12.0)),
                ("vwap", WeightPair::pass_only(8.0)),
                ("mtf_dominant", WeightPair::pass_only(12.0)),
                ("structure", WeightPair::new(14.0, -8.0)),
                ("smc_bonus", WeightPair::pass_only(4.0)),
                ("liquidity", WeightPair::pass_only(10.0)),
                ("volume", WeightPair::pass_only(5.0)),
                ("anti_trap", WeightPair::pass_only(6.0)),
                ("session", WeightPair::pass_only(15.0)),
                // Pine V58 Forex penalty stack.
                ("anti_chop", WeightPair::pass_only(-12.0)),
                ("strong_conflict", WeightPair::pass_only(-18.0)),
                ("macro_conflict", WeightPair::pass_only(-10.0)),
                ("vol_shock", WeightPair::pass_only(-4.0)),
            ],
        ),
        profile(
            AssetClass::StocksIdx,
            "volume_guard_first",
            [
                // IDX V5 — RVOL + benchmark + breadth
                ("bias_htf_4h", WeightPair::pass_only(10.0)),
                ("bias_htf_1d", WeightPair::pass_only(12.0)),
                ("trend_dir", WeightPair::pass_only(12.0)),
                ("trend_200", WeightPair::pass_only(14.0)),
                ("macd", WeightPair::pass_only(6.0)),
                ("rsi", WeightPair::pass_only(8.0)),
                ("dmi", WeightPair::pass_only(8.0)),
                ("adx_band", WeightPair::pass_only(10.0)),
                ("vwap", WeightPair::pass_only(8.0)),
                ("mtf_dominant", WeightPair::pass_only(10.0)),
                ("structure", WeightPair::new(12.0, -10.0)),
                ("smc_bonus", WeightPair::pass_only(4.0)),
                ("rvol_value_gate", WeightPair::pass_only(16.0)),
                ("ihsg_benchmark", WeightPair::pass_only(14.0)),
                ("cmf_obv", WeightPair::pass_only(12.0)),
                ("downside_risk", WeightPair::pass_only(6.0)),
                ("liquidity", WeightPair::pass_only(6.0)),
                ("anti_trap", WeightPair::pass_only(5.0)),
                ("session", WeightPair::pass_only(6.0)),
                ("support_resistance", WeightPair::pass_only(8.0)),
                // Pine V5 IDX penalty stack.
                ("anti_chop", WeightPair::pass_only(-12.0)),
                ("strong_conflict", WeightPair::pass_only(-18.0)),
                ("macro_conflict", WeightPair::pass_only(-10.0)),
                ("vol_shock", WeightPair::pass_only(-4.0)),
            ],
        ),
    ]
    .into_iter()
    .map(|p| (p.class, p))
    .collect()
}

fn profile(
    class: AssetClass,
    philosophy: &'static str,
    weights: impl IntoIterator<Item = (&'static str, WeightPair)>,
) -> AssetProfile {
    AssetProfile {
        class,
        philosophy,
        weights: weights.into_iter().collect(),
    }
}
