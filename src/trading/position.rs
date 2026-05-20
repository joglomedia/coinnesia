use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    pub symbol: String,
    pub quantity: Decimal,
    pub average_entry: Decimal,
}
