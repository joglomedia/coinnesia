pub fn fixed_pct_position_size(capital: f64, risk_pct: f64, risk_per_unit: f64) -> f64 {
    if risk_per_unit <= 0.0 {
        return 0.0;
    }
    capital * (risk_pct / 100.0) / risk_per_unit
}
