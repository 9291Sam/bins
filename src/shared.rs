use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::kalshi::{
    KalshiMarketDescriptor,
    KalshiMarketReader,
    KalshiMarketStatus,
    MarketPollState,
    MarketStreamEvent,
    MarketTicker
};

pub const MARKET_INTERVAL_MINUTES: usize = 15;
pub const MARKET_INTERVAL_SECONDS: usize = MARKET_INTERVAL_MINUTES * 60;

pub const SAVING_INTERVAL_HZ: usize = 4;
pub const SAVING_INTERVAL_SECONDS: f64 = 1.0 / (SAVING_INTERVAL_HZ as f64);

pub const DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE: usize =
    SAVING_INTERVAL_HZ * MARKET_INTERVAL_SECONDS;

pub type CompleteOrderBookRecord = Box<[Orderbook; DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE]>;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TickData
{
    pub official_bitcoin_price: Option<f64>,
    pub approx_bitcoin_price:   Option<f64>,
    pub market_mid_cents:       Option<f64>
}

pub type TickHistory = Box<[TickData; DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE]>;

/// Tapered-deci-cent#
#[derive(Clone)]
pub struct Orderbook
{
    // 0..=100   (101 slots) -> [$0.000, $0.100] (0.001 step)
    // 101..=179 (79 slots)  -> [$0.110, $0.890] (0.010 step)
    // 180..=280 (101 slots) -> [$0.900, $1.000] (0.001 step)
    pub data: [i32; 281]
}

impl Orderbook
{
    pub fn new() -> Orderbook
    {
        Orderbook {
            data: [0; 281]
        }
    }

    pub fn set_shares(&mut self, dollars: f64, shares: i32)
    {
        self.data[get_index_of_dollars(dollars)
            .with_context(|| format!("Tried to get index of ${dollars}"))
            .unwrap()] = shares;
    }

    pub fn add_shares(&mut self, dollars: f64, shares: i32)
    {
        self.data[get_index_of_dollars(dollars)
            .with_context(|| format!("Tried to get index of ${dollars}"))
            .unwrap()] += shares;
    }

    pub fn get_best_ask_dollars(&self) -> Option<f64>
    {
        self.data.iter().enumerate().find_map(|(idx, &shares)| {
            if shares < 0
            {
                index_to_dollars(idx)
            }
            else
            {
                None
            }
        })
    }

    pub fn get_best_bid_dollars(&self) -> Option<f64>
    {
        self.data
            .iter()
            .enumerate()
            .rev()
            .find_map(|(idx, &shares)| {
                if shares > 0
                {
                    index_to_dollars(idx)
                }
                else
                {
                    None
                }
            })
    }

    pub fn get_mid_cents(&self) -> Option<f64>
    {
        let ask = self.get_best_ask_dollars()?;
        let bid = self.get_best_bid_dollars()?;
        Some((ask + bid) / 2.0 * 100.0)
    }
}

pub fn get_index_of_dollars(dollars: f64) -> Option<usize>
{
    if !(0.0..=1.0).contains(&dollars)
    {
        return None;
    }

    let mils = (dollars * 1000.0).round() as i32;

    match mils
    {
        0..=100 => Some(mils as usize),
        110..=890 =>
        {
            if mils % 10 == 0
            {
                Some(101 + ((mils - 110) / 10) as usize)
            }
            else
            {
                None
            }
        }
        900..=1000 => Some(180 + (mils - 900) as usize),
        _ => None
    }
}

pub fn index_to_dollars(idx: usize) -> Option<f64>
{
    match idx
    {
        0..=100 => Some(idx as f64 / 1000.0),
        101..=179 => Some(0.110 + ((idx - 101) as f64 / 100.0)),
        180..=280 => Some(0.900 + ((idx - 180) as f64 / 1000.0)),
        _ => None
    }
}

pub struct MarketBundle
{
    pub communicator: KalshiMarketReader,

    pub orderbook:    Orderbook,
    pub tick_history: TickHistory,

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
        priv_key_path: String
    ) -> MarketBundle
    {
        MarketBundle {
            communicator: KalshiMarketReader::new(
                descriptor.ticker.clone(),
                state,
                api_key_id,
                priv_key_path
            ),
            orderbook:    Orderbook::new(),
            tick_history: vec![TickData::default(); DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
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
        self.close_time - Duration::from_mins(15)
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
