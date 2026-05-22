use crate::Candle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderBlockKind {
    Bullish,
    Bearish,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderBlock {
    pub kind: OrderBlockKind,
    pub low: f64,
    pub high: f64,
    pub valid: bool,
}

pub fn detect_order_blocks(
    candles: &[Candle],
    lookback: usize,
    min_displacement_body_ratio: f64,
) -> Vec<OrderBlock> {
    if candles.len() < 2 {
        return Vec::new();
    }

    let latest_close = candles.last().expect("len checked").close;
    let start = candles.len().saturating_sub(lookback.max(2));
    let mut blocks = Vec::new();

    for idx in (start + 1)..candles.len() {
        let displacement = &candles[idx];
        let base = &candles[idx - 1];
        let range = (displacement.high - displacement.low).max(f64::EPSILON);
        let body_ratio = displacement.body() / range;

        if body_ratio < min_displacement_body_ratio {
            continue;
        }

        if displacement.close > displacement.open && base.close < base.open {
            blocks.push(OrderBlock {
                kind: OrderBlockKind::Bullish,
                low: base.low,
                high: base.high,
                valid: latest_close >= base.low,
            });
        } else if displacement.close < displacement.open && base.close > base.open {
            blocks.push(OrderBlock {
                kind: OrderBlockKind::Bearish,
                low: base.low,
                high: base.high,
                valid: latest_close <= base.high,
            });
        }
    }

    blocks
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    #[test]
    fn detects_bullish_order_block_from_displacement() {
        let now = Utc::now();
        let candles = vec![
            candle(now, 100.0, 102.0, 94.0, 96.0),
            candle(now + Duration::minutes(1), 96.0, 112.0, 95.0, 111.0),
            candle(now + Duration::minutes(2), 111.0, 114.0, 108.0, 113.0),
        ];

        let blocks = detect_order_blocks(&candles, 3, 0.6);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].kind, OrderBlockKind::Bullish);
        assert!(blocks[0].valid);
        assert_eq!(blocks[0].low, 94.0);
    }

    fn candle(ts: chrono::DateTime<Utc>, open: f64, high: f64, low: f64, close: f64) -> Candle {
        Candle {
            ts,
            open,
            high,
            low,
            close,
            volume: 1_000.0,
        }
    }
}
