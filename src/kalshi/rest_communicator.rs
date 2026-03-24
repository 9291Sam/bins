const MARKETS_REST_API: &str = "https://api.elections.kalshi.com/trade-api/v2/markets";
use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum KalshiMarketStatus
{
    #[serde(rename = "initialized")]
    Initialized,
    #[serde(rename = "inactive")]
    Inactive,
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "determined")]
    Determined,
    #[serde(rename = "disputed")]
    Disputed,
    #[serde(rename = "amended")]
    Amended,
    #[serde(rename = "finalized")]
    Finalized
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
#[serde(transparent)]
pub struct MarketTicker(pub String);

#[derive(Debug, Clone, Deserialize)]
pub struct KalshiMarketDescriptor
{
    pub ticker:           MarketTicker,
    #[serde(rename = "floor_strike")]
    pub strike_price:     Option<f64>,
    pub close_time:       DateTime<Utc>,
    pub status:           KalshiMarketStatus,
    #[serde(
        default,
        deserialize_with = "super::deserialize_optional_stringified_float"
    )]
    pub expiration_value: Option<f64>
}

pub struct PreviousCurrentAndNextMarkets
{
    pub next_market:     KalshiMarketDescriptor,
    pub current_market:  KalshiMarketDescriptor,
    pub previous_market: KalshiMarketDescriptor
}

pub async fn poll_previous_current_and_next_market(
    client: &Client,
    target_time: DateTime<Utc>
) -> PreviousCurrentAndNextMarkets
{
    let mut markets = poll_nearby_markets(client, target_time).await;

    markets.sort_by_key(|l| l.close_time);

    let index_of_current_market = markets
        .iter()
        .enumerate()
        .find(|(_, m)| {
            let close_time = m.close_time;
            let start_time = close_time - Duration::from_mins(15);

            (start_time..close_time).contains(&target_time)
        })
        .map(|(idx, _)| idx)
        .expect("no market");

    let mut adjacent_markets =
        markets.drain(index_of_current_market - 1..=index_of_current_market + 1);

    PreviousCurrentAndNextMarkets {
        previous_market: adjacent_markets.next().unwrap(),
        current_market:  adjacent_markets.next().unwrap(),
        next_market:     adjacent_markets.next().unwrap()
    }
}

pub async fn poll_nearby_markets(
    client: &Client,
    target_time: DateTime<Utc>
) -> Vec<KalshiMarketDescriptor>
{
    let min_time = target_time - Duration::from_mins(30);
    let max_time = target_time + Duration::from_mins(30);

    #[derive(Debug, Deserialize)]
    struct KalshiMarketPollResult
    {
        markets: Vec<KalshiMarketDescriptor>
    }

    let response: KalshiMarketPollResult = client
        .get(MARKETS_REST_API)
        .query(&[
            ("series_ticker", "KXBTC15M"),
            ("min_close_ts", &min_time.timestamp().to_string()),
            ("max_close_ts", &max_time.timestamp().to_string()),
            ("limit", "10")
        ])
        .send()
        .await
        .context("Failed to send HTTP request to Kalshi")
        .unwrap()
        .json()
        .await
        .context("failed to parse market response into structure")
        .unwrap();

    response.markets
}
