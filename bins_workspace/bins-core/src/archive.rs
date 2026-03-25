use chrono::{DateTime, Utc};

use super::*;

#[derive(Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct MarketTick
{
    pub timestamp_ms:           i64,
    pub official_bitcoin_price: Option<f64>,
    pub approx_bitcoin_price:   Option<f64>,
    pub market_mid_cents:       Option<f64>,
    pub orderbook:              Orderbook
}

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
    pub fn save_to_disk(
        ticker: &str,
        close_time: DateTime<Utc>,
        strike_price: Option<f64>,
        final_price: Option<f64>,
        tick_history: &[MarketTick],
        directory: &str
    ) -> anyhow::Result<()>
    {
        let archive = MarketArchive {
            ticker:        ticker.to_string(),
            close_time_ts: close_time.timestamp(),
            strike_price:  strike_price.unwrap_or(0.0),
            final_price:   final_price.unwrap_or(0.0),
            tick_history:  tick_history.to_vec()
        };

        std::fs::create_dir_all(directory)?;

        let filename = format!("{}_{}.kalshi.rkyv", archive.ticker, archive.close_time_ts);
        let filepath = std::path::Path::new(directory).join(filename);

        let bytes = rkyv::to_bytes::<rkyv::rancor::BoxedError>(&archive)
            .map_err(|e| anyhow::anyhow!("Failed to serialize with rkyv: {}", e))?;

        let mut file = std::fs::File::create(filepath)?;
        std::io::Write::write_all(&mut file, &bytes)?;

        Ok(())
    }
}
