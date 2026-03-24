use std::sync::Arc;

use bins_core::{MARKET_INTERVAL_SECONDS, MarketTick, Orderbook};
use chrono::{DateTime, Utc};

use crate::{
    KalshiMarketDescriptor,
    KalshiMarketReader,
    KalshiMarketStatus,
    MarketPollState,
    MarketStreamEvent,
    MarketTicker
};

pub struct MarketBundle
{
    pub communicator: KalshiMarketReader,
    pub orderbook:    Orderbook,
    pub tick_history: Vec<MarketTick>,
    pub ticker:       MarketTicker,
    pub close_time:   DateTime<Utc>,
    pub strike_price: Option<f64>,
    pub final_price:  Option<f64>,
    pub status:       KalshiMarketStatus
}

impl MarketBundle
{
    pub fn new(
        descriptor: KalshiMarketDescriptor,
        state: MarketPollState,
        api_key_id: String,
        priv_key_path: String,
        on_update: Arc<dyn Fn() + Send + Sync>
    ) -> MarketBundle
    {
        MarketBundle {
            communicator: KalshiMarketReader::new(
                descriptor.ticker.clone(),
                state,
                api_key_id,
                priv_key_path,
                on_update
            ),
            orderbook:    Orderbook::new(),
            tick_history: Vec::new(),
            ticker:       descriptor.ticker,
            close_time:   descriptor.close_time,
            strike_price: descriptor.strike_price,
            final_price:  descriptor.expiration_value,
            status:       descriptor.status
        }
    }

    pub fn update_with_new_descriptor(&mut self, descriptor: KalshiMarketDescriptor)
    {
        self.ticker = descriptor.ticker;
        self.close_time = descriptor.close_time;
        self.strike_price = descriptor.strike_price;
        self.final_price = descriptor.expiration_value;
        self.status = descriptor.status;
    }

    pub fn get_start_time(&self) -> DateTime<Utc>
    {
        self.close_time - std::time::Duration::from_secs(MARKET_INTERVAL_SECONDS as u64)
    }

    pub fn apply_event(&mut self, event: MarketStreamEvent)
    {
        match event
        {
            MarketStreamEvent::OrderbookSnapshot(new_orderbook) => self.orderbook = new_orderbook,
            MarketStreamEvent::OrderbookDelta {
                price_dollars,
                size_delta
            } => self.orderbook.add_shares(price_dollars, size_delta),
            MarketStreamEvent::NewDescriptors(descriptors) =>
            {
                for d in descriptors.into_iter()
                {
                    if d.ticker == self.ticker
                    {
                        self.update_with_new_descriptor(d);
                    }
                }
            }
        }
    }
}
