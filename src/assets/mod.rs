pub mod altcoin;
pub mod btc;
pub mod forex;
pub mod gold;
pub mod stocks_idx;

use crate::{config::profiles, AssetClass};

pub fn philosophy(asset_class: AssetClass) -> &'static str {
    profiles::default_profiles()
        .get(&asset_class)
        .map(|profile| profile.philosophy)
        .unwrap_or("unknown")
}
