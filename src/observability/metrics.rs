#[derive(Debug, Clone, Copy, Default)]
pub struct RuntimeMetrics {
    pub scan_cycles: u64,
    pub symbols_scanned: u64,
    pub signals_generated: u64,
}
