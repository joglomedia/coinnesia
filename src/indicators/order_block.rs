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
