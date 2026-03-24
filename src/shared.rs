use std::fs::File;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use eframe::egui;
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

// The atomic event payload for sparse logging
#[derive(Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct MarketTick
{
    pub timestamp_ms:           i64,
    pub official_bitcoin_price: Option<f64>,
    pub approx_bitcoin_price:   Option<f64>,
    pub market_mid_cents:       Option<f64>,
    pub orderbook:              Orderbook
}

// rkyv archive format
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct MarketArchive
{
    pub ticker:        String,
    pub close_time_ts: i64,
    pub strike_price:  f64,
    pub final_price:   f64,
    pub tick_history:  Vec<MarketTick>
}

impl MarketArchive
{
    pub fn save_to_disk(bundle: &MarketBundle, directory: &str) -> Result<()>
    {
        let archive = MarketArchive {
            ticker:        bundle.ticker.0.clone(),
            close_time_ts: bundle.close_time.timestamp(),
            strike_price:  bundle.strike_price.unwrap_or(0.0),
            final_price:   bundle.final_price.unwrap_or(0.0),
            tick_history:  bundle.tick_history.clone()
        };

        std::fs::create_dir_all(directory)?;

        let filename = format!("{}_{}.kalshi.rkyv", archive.ticker, archive.close_time_ts);
        let filepath = Path::new(directory).join(filename);

        let bytes = rkyv::to_bytes::<rkyv::rancor::BoxedError>(&archive)
            .map_err(|e| anyhow::anyhow!("Failed to serialize with rkyv: {}", e))?;

        let mut file = File::create(filepath)?;
        file.write_all(&bytes)?;

        Ok(())
    }
}

/// Tapered-deci-cent#
#[derive(Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
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
        ctx: egui::Context
    ) -> MarketBundle
    {
        MarketBundle {
            communicator: KalshiMarketReader::new(
                descriptor.ticker.clone(),
                state,
                api_key_id,
                priv_key_path,
                ctx
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
