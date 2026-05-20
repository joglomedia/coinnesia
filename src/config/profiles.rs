use std::collections::BTreeMap;

use crate::AssetClass;

#[derive(Debug, Clone, PartialEq)]
pub struct AssetProfile {
    pub class: AssetClass,
    pub philosophy: &'static str,
    pub weights: BTreeMap<&'static str, f64>,
}

pub fn default_profiles() -> BTreeMap<AssetClass, AssetProfile> {
    [
        profile(
            AssetClass::Btc,
            "structure_first",
            [
                ("structure", 18.0),
                ("ema", 15.0),
                ("vwap", 12.0),
                ("adx_dmi", 12.0),
                ("volume", 12.0),
                ("rsi", 10.0),
                ("macd", 8.0),
                ("liquidity", 8.0),
                ("support_resistance", 5.0),
            ],
        ),
        profile(
            AssetClass::Altcoin,
            "anti_trap_first",
            [
                ("ltf_consensus", 16.0),
                ("wick_chaos", 15.0),
                ("volume", 14.0),
                ("atr_expansion", 12.0),
                ("liquidity", 12.0),
                ("vwap", 10.0),
                ("structure", 8.0),
                ("ema", 7.0),
                ("momentum", 6.0),
            ],
        ),
        profile(
            AssetClass::Gold,
            "proxy_session_first",
            [
                ("xauusd_proxy", 20.0),
                ("ema_htf", 15.0),
                ("session", 14.0),
                ("vwap", 12.0),
                ("atr_news", 10.0),
                ("structure", 10.0),
                ("liquidity", 8.0),
                ("momentum", 6.0),
                ("token_volume", 5.0),
            ],
        ),
        profile(
            AssetClass::Forex,
            "session_rr_first",
            [
                ("htf_bias", 18.0),
                ("session", 15.0),
                ("structure", 14.0),
                ("adx", 12.0),
                ("ema", 12.0),
                ("liquidity", 10.0),
                ("rsi", 8.0),
                ("macd", 6.0),
                ("tick_volume", 5.0),
            ],
        ),
        profile(
            AssetClass::StocksIdx,
            "volume_guard_first",
            [
                ("rvol_value_gate", 16.0),
                ("ihsg_benchmark", 14.0),
                ("ema", 14.0),
                ("structure", 12.0),
                ("cmf_obv", 12.0),
                ("adx", 10.0),
                ("rsi", 8.0),
                ("support_resistance", 8.0),
                ("downside_risk", 6.0),
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
    weights: impl IntoIterator<Item = (&'static str, f64)>,
) -> AssetProfile {
    AssetProfile {
        class,
        philosophy,
        weights: weights.into_iter().collect(),
    }
}
