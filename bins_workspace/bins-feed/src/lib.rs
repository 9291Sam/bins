pub mod bitcoin_price_grabber;
pub mod bundle;
pub mod market_reader;
pub mod rest_communicator;

pub use bitcoin_price_grabber::{BitcoinPriceGrabber, BitcoinPriceUpdate};
pub use bundle::MarketBundle;
pub use market_reader::{KalshiMarketReader, MarketPollState, MarketStreamEvent};
pub use rest_communicator::{
    KalshiMarketDescriptor,
    KalshiMarketStatus,
    MarketTicker,
    PreviousCurrentAndNextMarkets,
    poll_nearby_markets,
    poll_previous_current_and_next_market
};
use serde::{Deserialize, Deserializer};

fn deserialize_optional_stringified_float<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>
{
    match Option::<String>::deserialize(deserializer)?
    {
        Some(s) if s.is_empty() => Ok(None),
        Some(s) => s.parse::<f64>().map(Some).map_err(serde::de::Error::custom),
        None => Ok(None)
    }
}

fn deserialize_stringified_float<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>
{
    match &*String::deserialize(deserializer)?
    {
        "" => Err(serde::de::Error::custom("")),
        s => s.parse::<f64>().map_err(serde::de::Error::custom)
    }
}
