//! Altcoin V62 panel parity. The fixture covers a thin/illiquid mid-cap alt
//! with elevated wick noise — drives the V62 adaptive engine through the
//! same code path the live evaluator would take on PAXG/MATIC class symbols.

use chrono::Duration;
use coinnesia::AssetClass;

use super::common;

#[test]
fn parity_altcoin_1d_v62() {
    let candles = common::synthesize_candles(
        160,
        common::TrendDirection::Long,
        2.50,
        0.035,
        800.0,
        Duration::days(1),
    );
    let cfg = common::config_with_symbol("PAXGUSDT", AssetClass::Altcoin);
    let (_, panel) = common::evaluate(&cfg, "PAXGUSDT", &candles);
    assert_eq!(panel.asset_class, AssetClass::Altcoin);
    common::assert_panel_matches("altcoin_v62", &panel);
}
