use crate::Candle;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandleShape {
    pub body: f64,
    pub upper_wick: f64,
    pub lower_wick: f64,
    pub range: f64,
    pub close_location_value: f64,
    pub body_ratio: f64,
    pub upper_wick_ratio: f64,
    pub lower_wick_ratio: f64,
    pub bias: CandleBias,
    pub trap_risk: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandleBias {
    Bullish,
    Bearish,
    Neutral,
}

pub fn analyze(candle: &Candle) -> CandleShape {
    let high_body = candle.open.max(candle.close);
    let low_body = candle.open.min(candle.close);
    let range = (candle.high - candle.low).max(0.0);
    let body = candle.body();
    let upper_wick = (candle.high - high_body).max(0.0);
    let lower_wick = (low_body - candle.low).max(0.0);
    let close_location_value = if range == 0.0 {
        0.0
    } else {
        ((candle.close - candle.low) - (candle.high - candle.close)) / range
    };
    let body_ratio = ratio(body, range);
    let upper_wick_ratio = ratio(upper_wick, range);
    let lower_wick_ratio = ratio(lower_wick, range);
    let bias = if candle.close > candle.open {
        CandleBias::Bullish
    } else if candle.close < candle.open {
        CandleBias::Bearish
    } else {
        CandleBias::Neutral
    };
    let trap_risk = upper_wick_ratio >= 0.45 || lower_wick_ratio >= 0.45;

    CandleShape {
        body,
        upper_wick,
        lower_wick,
        range,
        close_location_value,
        body_ratio,
        upper_wick_ratio,
        lower_wick_ratio,
        bias,
        trap_risk,
    }
}

fn ratio(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn candle_shape_reports_body_wicks_clv_and_trap_risk() {
        let shape = analyze(&Candle {
            ts: Utc::now(),
            open: 100.0,
            high: 112.0,
            low: 99.0,
            close: 102.0,
            volume: 1_000.0,
        });

        assert_eq!(shape.bias, CandleBias::Bullish);
        assert_eq!(shape.body, 2.0);
        assert_eq!(shape.upper_wick, 10.0);
        assert!(shape.trap_risk);
        assert!(shape.close_location_value < 0.0);
    }
}
