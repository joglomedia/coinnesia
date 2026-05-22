use chrono::{DateTime, Timelike, Utc};
use chrono_tz::Asia::Jakarta;

use crate::AssetClass;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketSession {
    Asia,
    Europe,
    Usa,
    Idx,
    RolloverAvoid,
    Closed,
}

pub fn classify_wib(ts: DateTime<Utc>) -> MarketSession {
    let local = ts.with_timezone(&Jakarta);
    let minutes = local.hour() * 60 + local.minute();

    if in_range(minutes, 4 * 60 + 55, 6 * 60 + 10) {
        MarketSession::RolloverAvoid
    } else if in_range(minutes, 9 * 60, 15 * 60) {
        MarketSession::Idx
    } else if in_range(minutes, 19 * 60, 3 * 60) {
        MarketSession::Usa
    } else if in_range(minutes, 14 * 60, 22 * 60) {
        MarketSession::Europe
    } else if in_range(minutes, 6 * 60, 14 * 60) {
        MarketSession::Asia
    } else {
        MarketSession::Closed
    }
}

fn in_range(value: u32, start: u32, end: u32) -> bool {
    if start <= end {
        value >= start && value < end
    } else {
        value >= start || value < end
    }
}

pub fn session_allows_asset(session: MarketSession, asset_class: AssetClass) -> bool {
    match asset_class {
        AssetClass::StocksIdx => session == MarketSession::Idx,
        AssetClass::Forex => matches!(session, MarketSession::Europe | MarketSession::Usa),
        AssetClass::Gold => matches!(session, MarketSession::Europe | MarketSession::Usa),
        AssetClass::Btc | AssetClass::Altcoin => !matches!(session, MarketSession::RolloverAvoid),
        AssetClass::StocksUs => session == MarketSession::Usa,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn session_gate_is_asset_specific() {
        assert!(session_allows_asset(MarketSession::Usa, AssetClass::Btc));
        assert!(session_allows_asset(
            MarketSession::Europe,
            AssetClass::Forex
        ));
        assert!(!session_allows_asset(
            MarketSession::Asia,
            AssetClass::Forex
        ));
        assert!(session_allows_asset(
            MarketSession::Idx,
            AssetClass::StocksIdx
        ));
    }

    #[test]
    fn classifies_rollover_in_wib() {
        let ts = Utc.with_ymd_and_hms(2026, 5, 20, 22, 0, 0).unwrap();
        assert_eq!(classify_wib(ts), MarketSession::RolloverAvoid);
    }
}
