#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneKind {
    Support,
    Resistance,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriceZone {
    pub kind: ZoneKind,
    pub low: f64,
    pub high: f64,
    pub strength: f64,
}
