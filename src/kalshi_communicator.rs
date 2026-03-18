const MARKETS_REST_API: &str = "https://api.elections.kalshi.com/trade-api/v2/markets";
const TRADE_API_SOCKET: &str = "wss://api.elections.kalshi.com/trade-api/ws/v2";

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use reqwest::Client;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json, to_string_pretty};
use tokio::net::TcpStream;

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub enum KalshiBinaryMarketResult
{
    #[serde(rename = "yes")]
    Yes,
    #[serde(rename = "no")]
    No,
    #[serde(rename = "")]
    Unresolved
}

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

#[derive(Debug, Clone, Deserialize)]
pub struct KalshiMarket
{
    event_ticker:     String, // the url bit
    #[serde(rename = "floor_strike")]
    strike_price:     Option<serde_json::Value>,
    close_time:       DateTime<Utc>,
    status:           KalshiMarketStatus,
    result:           Option<KalshiBinaryMarketResult>,
    #[serde(default, deserialize_with = "deserialize_optional_stringified_float")]
    expiration_value: Option<f64>
}

pub async fn poll_nearby_markets(client: &Client, target_time: DateTime<Utc>) -> Vec<KalshiMarket>
{
    let min_time = target_time - Duration::minutes(30);
    let max_time = target_time + Duration::minutes(30);

    #[derive(Debug, Deserialize)]
    struct KalshiMarketPollResult
    {
        markets: Vec<KalshiMarket>
    }

    let response: KalshiMarketPollResult = client
        .get("https://api.elections.kalshi.com/trade-api/v2/markets")
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
