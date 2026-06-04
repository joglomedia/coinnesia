//! BTC V61.9 panel parity on a captured 1D series.
//!
//! Synthetic bullish trend that exercises the structure → break → emission
//! path. The captured `btc_v619.panel.json` fixture pins every field of the
//! resulting `PanelReport` so future Pine updates surface as a JSON diff.

use chrono::Duration;
use coinnesia::AssetClass;

use super::common;

#[test]
fn parity_btc_1d_v619() {
    let candles = common::synthesize_candles(
        180,
        common::TrendDirection::Long,
        30_000.0,
        320.0,
        1_500.0,
        Duration::days(1),
    );
    let cfg = common::config_with_symbol("BTCUSDT", AssetClass::Btc);
    let (_, panel) = common::evaluate(&cfg, "BTCUSDT", &candles);
    assert_eq!(panel.asset_class, AssetClass::Btc);
    common::assert_panel_matches("btc_v619", &panel);
}
