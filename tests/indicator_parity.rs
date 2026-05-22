use approx::assert_relative_eq;
use chrono::{DateTime, Utc};
use coinnesia::{
    indicators::{
        atr::calculate_atr,
        rsi::calculate_rsi,
        vwap::{anchored_vwap, daily_vwap_wib},
    },
    Candle,
};

#[test]
fn fixture_matches_tradingview_rma_rsi_reference_values() {
    let candles = fixture_candles();
    let closes = candles
        .iter()
        .map(|candle| candle.close)
        .collect::<Vec<_>>();
    let rsi = calculate_rsi(&closes, 14);

    assert_relative_eq!(rsi[14].value, 70.46413502109705, epsilon = 1e-10);
    assert_relative_eq!(rsi[15].value, 66.24961855355505, epsilon = 1e-10);
    assert_relative_eq!(rsi[16].value, 66.48094183471265, epsilon = 1e-10);
}

#[test]
fn fixture_keeps_atr_ready_at_wilder_seed_bar() {
    let candles = fixture_candles();
    let atr = calculate_atr(&candles, 14);

    assert!(!atr[12].ready);
    assert!(atr[13].ready);
    assert_relative_eq!(atr[13].value, 2.0, epsilon = 1e-10);
    assert_relative_eq!(atr[16].value, 2.0, epsilon = 1e-10);
}

#[test]
fn fixture_vwap_continuous_and_daily_are_equal_for_single_day() {
    let candles = fixture_candles();
    let continuous = anchored_vwap(&candles);
    let daily = daily_vwap_wib(&candles);

    assert_relative_eq!(
        continuous.last().unwrap().value,
        daily.last().unwrap().value,
        epsilon = 1e-10
    );
}

fn fixture_candles() -> Vec<Candle> {
    include_str!("fixtures/tradingview_indicator_fixture.csv")
        .lines()
        .skip(1)
        .map(|line| {
            let columns = line.split(',').collect::<Vec<_>>();
            Candle {
                ts: columns[0]
                    .parse::<DateTime<Utc>>()
                    .expect("fixture timestamp parses"),
                open: columns[1].parse().expect("fixture open parses"),
                high: columns[2].parse().expect("fixture high parses"),
                low: columns[3].parse().expect("fixture low parses"),
                close: columns[4].parse().expect("fixture close parses"),
                volume: columns[5].parse().expect("fixture volume parses"),
            }
        })
        .collect()
}
