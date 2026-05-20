use crate::Candle;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandleShape {
    pub body: f64,
    pub upper_wick: f64,
    pub lower_wick: f64,
    pub range: f64,
    pub close_location_value: f64,
}

pub fn analyze(candle: &Candle) -> CandleShape {
    let high_body = candle.open.max(candle.close);
    let low_body = candle.open.min(candle.close);
    let range = (candle.high - candle.low).max(0.0);
    let close_location_value = if range == 0.0 {
        0.0
    } else {
        ((candle.close - candle.low) - (candle.high - candle.close)) / range
    };

    CandleShape {
        body: candle.body(),
        upper_wick: (candle.high - high_body).max(0.0),
        lower_wick: (low_body - candle.low).max(0.0),
        range,
        close_location_value,
    }
}
