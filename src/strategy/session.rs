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
}

pub fn classify_wib(ts: DateTime<Utc>) -> MarketSession {
    let local = ts.with_timezone(&Jakarta);
    let minutes = local.hour() * 60 + local.minute();

    // Coinnesia overlays take precedence over Pine's three-session classification
    // because they drive asset-specific gating that Pine does not model.
    if in_range(minutes, 4 * 60 + 55, 6 * 60 + 10) {
        return MarketSession::RolloverAvoid;
    }
    if in_range(minutes, 9 * 60, 15 * 60) {
        return MarketSession::Idx;
    }

    // Pine BTC V61.9 `f_session_from_minutes`:
    //   ASIA  07:00-14:59 (420-899)
    //   EROPA 15:00-20:29 (900-1229)
    //   USA   else (covers 20:30-06:59, including the cross-midnight window)
    if minutes >= 7 * 60 && minutes < 15 * 60 {
        MarketSession::Asia
    } else if minutes >= 15 * 60 && minutes < 20 * 60 + 30 {
        MarketSession::Europe
    } else {
        MarketSession::Usa
    }
}

/// Display string matching Pine's `f_session_time_range` for the SESI panel row.
/// Pine itself only models Asia/Europe/Usa; the Coinnesia overlays (Idx,
/// RolloverAvoid) get their own labels because the panel must report them.
pub fn session_time_range_string(session: MarketSession) -> &'static str {
    match session {
        MarketSession::Asia => "07:00-14:59 WIB",
        MarketSession::Europe => "15:00-20:29 WIB",
        MarketSession::Usa => "20:30-02:59 WIB",
        MarketSession::Idx => "09:00-15:00 WIB",
        MarketSession::RolloverAvoid => "04:55-06:10 WIB",
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

    fn at_wib(hour: u32, minute: u32) -> DateTime<Utc> {
        // WIB is UTC+7, so subtract 7 hours to get the UTC instant.
        let h = hour as i32 - 7;
        let (day, h) = if h < 0 { (20, h + 24) } else { (21, h) };
        Utc.with_ymd_and_hms(2026, 5, day, h as u32, minute, 0).unwrap()
    }

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

    #[test]
    fn classify_wib_at_pine_boundaries() {
        // 06:59 WIB → Pine USA (else branch; before Asia starts at 07:00)
        assert_eq!(classify_wib(at_wib(6, 59)), MarketSession::Usa);
        // 07:00 WIB → Asia start (07:00 not in Idx 09:00-15:00, not in Rollover)
        assert_eq!(classify_wib(at_wib(7, 0)), MarketSession::Asia);
        // 14:59 WIB → Idx overlay takes precedence (09:00-15:00 window)
        assert_eq!(classify_wib(at_wib(14, 59)), MarketSession::Idx);
        // 15:00 WIB → Idx ends (exclusive); Pine EROPA starts here
        assert_eq!(classify_wib(at_wib(15, 0)), MarketSession::Europe);
        // 19:00 WIB → still inside Pine EROPA (15:00-20:29)
        assert_eq!(classify_wib(at_wib(19, 0)), MarketSession::Europe);
        // 20:30 WIB → Pine USA start
        assert_eq!(classify_wib(at_wib(20, 30)), MarketSession::Usa);
    }

    #[test]
    fn classify_wib_without_overlay_returns_asia_inside_pine_window() {
        // 08:59 WIB — past Asia start (07:00) but before Idx overlay (09:00).
        assert_eq!(classify_wib(at_wib(8, 59)), MarketSession::Asia);
    }

    #[test]
    fn session_time_range_strings_match_pine_panel_labels() {
        assert_eq!(
            session_time_range_string(MarketSession::Asia),
            "07:00-14:59 WIB"
        );
        assert_eq!(
            session_time_range_string(MarketSession::Europe),
            "15:00-20:29 WIB"
        );
        assert_eq!(
            session_time_range_string(MarketSession::Usa),
            "20:30-02:59 WIB"
        );
        assert_eq!(
            session_time_range_string(MarketSession::Idx),
            "09:00-15:00 WIB"
        );
        assert_eq!(
            session_time_range_string(MarketSession::RolloverAvoid),
            "04:55-06:10 WIB"
        );
    }
}
