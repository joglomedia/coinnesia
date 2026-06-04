//! Gold V1 panel parity. Exercises the Gold evaluator branch — without a
//! proxy snapshot the XAU bias row collapses to `None` and the panel falls
//! back to the session/HTF view.

use chrono::Duration;
use coinnesia::AssetClass;

use super::common;

#[test]
fn parity_gold_1d_v1() {
    let candles = common::synthesize_candles(
        160,
        common::TrendDirection::Sideways,
        2_050.0,
        12.0,
        450.0,
        Duration::days(1),
    );
    let cfg = common::config_with_symbol("XAUUSD", AssetClass::Gold);
    let (_, panel) = common::evaluate(&cfg, "XAUUSD", &candles);
    assert_eq!(panel.asset_class, AssetClass::Gold);
    common::assert_panel_matches("gold_v1", &panel);
}
