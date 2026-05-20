pub mod allocator;
pub mod balance;
pub mod exposure;
pub mod rebalancer;

use crate::config::PortfolioConfig;

pub struct PortfolioManager {
    pub config: PortfolioConfig,
}

impl PortfolioManager {
    pub const fn new(config: PortfolioConfig) -> Self {
        Self { config }
    }
}
