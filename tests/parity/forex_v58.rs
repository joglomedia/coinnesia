//! Forex V58 panel parity. EURUSD-style price level + tight ATR.

use chrono::Duration;
use coinnesia::AssetClass;

use super::common;

#[test]
fn parity_forex_1d_v58() {
    let candles = common::synthesize_candles(
        160,
        common::TrendDirection::Short,
        1.0850,
        0.0040,
        2_500.0,
        Duration::days(1),
    );
    let cfg = common::config_with_symbol("EURUSD", AssetClass::Forex);
    let (_, panel) = common::evaluate(&cfg, "EURUSD", &candles);
    assert_eq!(panel.asset_class, AssetClass::Forex);
    common::assert_panel_matches("forex_v58", &panel);
}
