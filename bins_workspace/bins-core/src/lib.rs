mod archive;
mod orderbook;

pub use archive::{MarketArchive, MarketTick};
pub use orderbook::{Orderbook, get_index_of_dollars, index_to_dollars};

pub const MARKET_INTERVAL_MINUTES: usize = 15;
pub const MARKET_INTERVAL_SECONDS: usize = MARKET_INTERVAL_MINUTES * 60;
