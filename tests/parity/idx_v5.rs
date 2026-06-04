//! Stocks IDX V5 panel parity. IHSG composite — exercises the StocksIdx
//! evaluator branch even without proxy/CMF data threaded in.

use chrono::Duration;
use coinnesia::AssetClass;

use super::common;

#[test]
fn parity_idx_1d_v5() {
    let candles = common::synthesize_candles(
        160,
        common::TrendDirection::Long,
        7_300.0,
        45.0,
        600_000_000.0,
        Duration::days(1),
    );
    let cfg = common::config_with_symbol("IDX:COMPOSITE", AssetClass::StocksIdx);
    let (_, panel) = common::evaluate(&cfg, "IDX:COMPOSITE", &candles);
    assert_eq!(panel.asset_class, AssetClass::StocksIdx);
    common::assert_panel_matches("idx_v5", &panel);
}
