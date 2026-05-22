use crate::Candle;

use super::{
    adx::DmiPoint,
    atr::calculate_atr,
    candle::{analyze, CandleBias},
    ema::calculate_ema,
    rsi::calculate_rsi,
    IndicatorPoint,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketRegime {
    Sideways,
    TrendExpansion,
    DistributionRisk,
    AccumulationRisk,
    Shock,
}

impl MarketRegime {
    pub const fn allows_signals(self) -> bool {
        matches!(self, Self::TrendExpansion)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RegimeConfig {
    pub adx_chop: f64,
    pub adx_trend: f64,
    pub shock_atr_multiple: f64,
    pub ema_flat_pct: f64,
}

impl Default for RegimeConfig {
    fn default() -> Self {
        Self {
            adx_chop: 15.0,
            adx_trend: 22.0,
            shock_atr_multiple: 3.0,
            ema_flat_pct: 0.001,
        }
    }
}

pub fn classify_regime(
    candles: &[Candle],
    dmi: &[DmiPoint],
    rsi: &[IndicatorPoint],
    config: RegimeConfig,
) -> MarketRegime {
    if candles.len() < 2 {
        return MarketRegime::Sideways;
    }

    let latest = candles.last().expect("len checked");
    if latest.high < latest.low || latest.open <= 0.0 || latest.close <= 0.0 {
        return MarketRegime::Shock;
    }

    let atr = calculate_atr(candles, 14);
    if let Some(atr) = atr.last().filter(|point| point.ready) {
        let range = latest.high - latest.low;
        if atr.value > 0.0 && range / atr.value >= config.shock_atr_multiple {
            return MarketRegime::Shock;
        }
    }

    let shape = analyze(latest);
    let latest_dmi = dmi.last().copied();
    let latest_rsi = rsi.last().copied();

    if repeated_wick_bias(candles, CandleBias::Bearish) {
        return MarketRegime::DistributionRisk;
    }
    if repeated_wick_bias(candles, CandleBias::Bullish) {
        return MarketRegime::AccumulationRisk;
    }

    if let Some(dmi) = latest_dmi {
        if dmi.adx.ready && dmi.adx.value >= config.adx_trend && shape.body_ratio >= 0.45 {
            return MarketRegime::TrendExpansion;
        }

        if dmi.adx.ready && dmi.adx.value <= config.adx_chop {
            return MarketRegime::Sideways;
        }
    }

    if let Some(rsi) = latest_rsi {
        if rsi.ready && (45.0..=55.0).contains(&rsi.value) && ema_is_flat(candles, config) {
            return MarketRegime::Sideways;
        }
    }

    MarketRegime::Sideways
}

pub fn classify_from_candles(candles: &[Candle]) -> MarketRegime {
    let closes = candles
        .iter()
        .map(|candle| candle.close)
        .collect::<Vec<_>>();
    let dmi = super::adx::calculate_dmi(candles, 14, 14);
    let rsi = calculate_rsi(&closes, 14);
    classify_regime(candles, &dmi, &rsi, RegimeConfig::default())
}

fn ema_is_flat(candles: &[Candle], config: RegimeConfig) -> bool {
    let closes = candles
        .iter()
        .map(|candle| candle.close)
        .collect::<Vec<_>>();
    let ema = calculate_ema(closes, 20);
    let Some((latest, previous)) = ema
        .iter()
        .rev()
        .filter(|point| point.ready)
        .take(2)
        .collect::<Vec<_>>()
        .split_first()
        .and_then(|(latest, rest)| rest.first().map(|previous| (*latest, *previous)))
    else {
        return false;
    };

    previous.value != 0.0
        && ((latest.value - previous.value).abs() / previous.value) <= config.ema_flat_pct
}

fn repeated_wick_bias(candles: &[Candle], bias: CandleBias) -> bool {
    let count = candles
        .iter()
        .rev()
        .take(5)
        .filter(|candle| {
            let shape = analyze(candle);
            match bias {
                CandleBias::Bearish => shape.upper_wick_ratio >= 0.45,
                CandleBias::Bullish => shape.lower_wick_ratio >= 0.45,
                CandleBias::Neutral => false,
            }
        })
        .count();
    count >= 3
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    #[test]
    fn classifies_shock_when_range_is_large_vs_atr() {
        let now = Utc::now();
        let mut candles = (0..20)
            .map(|idx| Candle {
                ts: now + Duration::minutes(idx),
                open: 100.0,
                high: 102.0,
                low: 98.0,
                close: 101.0,
                volume: 1_000.0,
            })
            .collect::<Vec<_>>();
        candles.push(Candle {
            ts: now + Duration::minutes(21),
            open: 101.0,
            high: 140.0,
            low: 80.0,
            close: 120.0,
            volume: 5_000.0,
        });

        assert_eq!(classify_from_candles(&candles), MarketRegime::Shock);
    }

    #[test]
    fn classifies_trend_expansion_with_strong_adx_and_body() {
        let now = Utc::now();
        let candles = vec![
            Candle {
                ts: now,
                open: 98.0,
                high: 101.0,
                low: 97.0,
                close: 100.0,
                volume: 1_000.0,
            },
            Candle {
                ts: now + Duration::minutes(1),
                open: 100.0,
                high: 112.0,
                low: 99.0,
                close: 111.0,
                volume: 1_000.0,
            },
        ];
        let dmi = vec![DmiPoint {
            adx: IndicatorPoint::ready(30.0),
            di_plus: IndicatorPoint::ready(25.0),
            di_minus: IndicatorPoint::ready(10.0),
        }];
        let rsi = vec![IndicatorPoint::ready(65.0)];

        assert_eq!(
            classify_regime(&candles, &dmi, &rsi, RegimeConfig::default()),
            MarketRegime::TrendExpansion
        );
    }
}
