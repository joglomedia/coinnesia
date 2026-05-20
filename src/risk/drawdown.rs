#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawdownState {
    pub current_pct: f64,
    pub critical: bool,
}
