use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioBalance {
    pub available: Decimal,
    pub locked: Decimal,
}
