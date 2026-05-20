use crate::{config::AllocationConfig, AssetClass};

pub fn allocation_pct(config: &AllocationConfig, asset_class: AssetClass) -> f64 {
    match asset_class {
        AssetClass::Btc => config.btc_pct,
        AssetClass::Altcoin => config.altcoin_pct,
        AssetClass::Gold => config.gold_pct,
        AssetClass::Forex => config.forex_pct,
        AssetClass::StocksIdx | AssetClass::StocksUs => config.stocks_pct,
    }
}
