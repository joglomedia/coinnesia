use chrono::{DateTime, Timelike, Utc};
use chrono_tz::Asia::Jakarta;

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
