const MARKETS_REST_API: &str = "https://api.elections.kalshi.com/trade-api/v2/markets";
const TRADE_API_SOCKET: &str = "wss://api.elections.kalshi.com/trade-api/ws/v2";

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use futures_util::sink::SinkExt;
use reqwest::Client;
use rsa::RsaPrivateKey;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pss::BlindedSigningKey;
use rsa::rand_core::OsRng;
use rsa::signature::{RandomizedSigner, SignatureEncoding};
use serde::{Deserialize, Deserializer};
use serde_json::json;
use sha2::Sha256;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::interval;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
#[serde(transparent)]
pub struct MarketTicker(pub String);

// https://docs.kalshi.com/api-reference/market/get-market
#[derive(Debug, Clone, Deserialize)]
pub struct KalshiMarketDescriptor
{
    pub ticker:           MarketTicker,
    #[serde(rename = "floor_strike")]
    pub strike_price:     Option<f64>,
    pub close_time:       DateTime<Utc>,
    pub status:           KalshiMarketStatus,
    pub result:           Option<KalshiBinaryMarketResult>,
    #[serde(default, deserialize_with = "deserialize_optional_stringified_float")]
    pub expiration_value: Option<f64>
}

impl KalshiMarketDescriptor
{
    pub fn get_start_time(&self) -> DateTime<Utc>
    {
        self.close_time - Duration::from_mins(15)
    }
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

pub async fn connect_ws(
    tickers: &[String],
    api_key_id: &str,
    priv_key_path: &str
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>>
{
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .to_string();
    let private_key = RsaPrivateKey::read_pkcs1_pem_file(priv_key_path).context("Bad RSA key")?;
    let signature = BlindedSigningKey::<Sha256>::new(private_key).sign_with_rng(
        &mut OsRng,
        format!("{}GET/trade-api/ws/v2", timestamp).as_bytes()
    );

    let mut req = TRADE_API_SOCKET.into_client_request()?;
    let h = req.headers_mut();
    h.insert("KALSHI-ACCESS-KEY", HeaderValue::from_str(api_key_id)?);
    h.insert(
        "KALSHI-ACCESS-SIGNATURE",
        HeaderValue::from_str(&BASE64.encode(signature.to_vec()))?
    );
    h.insert(
        "KALSHI-ACCESS-TIMESTAMP",
        HeaderValue::from_str(&timestamp)?
    );

    let (mut ws, _) = connect_async(req).await.context("WS connect failed")?;
    ws.send(Message::Text(
        json!({
            "id": 1,
            "cmd": "subscribe",
            "params": {
                "channels": ["orderbook_delta"],
                "market_tickers": tickers
            }
        })
        .to_string()
        .into()
    ))
    .await?;
    Ok(ws)
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MarketPollState
{
    FarBeforeActive,   // in the 30 minutes before a market becomes active,
    RightBeforeActive, // in the 30 seconds before a market becomes active,
    Active,            // The market is active,
    ActivelyTryingToResolve, /* the market has finished and so has trading, we are trying to
                        * figure out what the final strike price is */
    Resolved
}

pub enum MarketStreamEvent
{
    OrderbookSnapshot(crate::OrderBookShares),
    OrderbookDelta
    {
        price_cents: u8,
        size_delta:  i32
    },
    Resolved
    {
        final_price: f64
    },
    FatalNetworkError(String)
}

pub struct KalshiMarketReader
{
    state_cache: MarketPollState,
    input_tx:    UnboundedSender<MarketPollState>,
    output_rx:   UnboundedReceiver<MarketStreamEvent>
}

impl KalshiMarketReader
{
    pub fn new(
        ticker: MarketTicker,
        initial_state: MarketPollState,
        api_key_id: String,
        priv_key_path: String
    ) -> KalshiMarketReader
    {
        let (input_tx, mut input_rx) = unbounded_channel();
        let (output_tx, output_rx) = unbounded_channel();

        let task_state = initial_state.clone();

        tokio::task::spawn(async move {
            let ticker = ticker;
            let api_key_id = api_key_id;
            let priv_key_path = priv_key_path;
            let mut state = task_state;

            const TIME_BETWEEN_RESOLVE_POLLS_MS: u64 = 1000;
            const TIME_BETWEEN_TICKS_MS: u64 = 25;
            const TICKS_BETWEEN_RESOLVE_POLLS: u64 =
                TIME_BETWEEN_RESOLVE_POLLS_MS / TIME_BETWEEN_TICKS_MS;

            let mut interval = interval(Duration::from_millis(TIME_BETWEEN_TICKS_MS));
            let mut ticks_since_last_resolve_poll = 0;

            let rest_client: Client = Client::new();
            let mut web_socket: Option<WebSocketStream<MaybeTlsStream<TcpStream>>> = None;

            loop
            {
                tokio::select! {
                    v = input_rx.recv() => {
                        match v
                        {
                            Some(new_state) => state = new_state,
                            None => break
                        }
                    },
                    _ = interval.tick() => {

                        match state {
                            MarketPollState::FarBeforeActive => {},
                            MarketPollState::RightBeforeActive | MarketPollState::Active => {
                                if web_socket.is_none()
                                {
                                    web_socket = Some(
                                        connect_ws(
                                            std::slice::from_ref(&ticker.0),
                                            &api_key_id,
                                            &priv_key_path
                                        ).await.unwrap()
                                    );

                                    // println!("connected web socket");
                                }
                            },
                            MarketPollState::ActivelyTryingToResolve => {
                                ticks_since_last_resolve_poll += 1;

                                if ticks_since_last_resolve_poll > TICKS_BETWEEN_RESOLVE_POLLS
                                {
                                    ticks_since_last_resolve_poll = 0;

                                    let now = Utc::now();

                                    let nearby_markets = poll_nearby_markets(
                                        &rest_client, now
                                    ).await;

                                    let this_market_current_data = nearby_markets
                                        .iter()
                                        .find(|m| m.ticker == ticker)
                                        .unwrap();

                                    if let Some(strike_price) =
                                        this_market_current_data.strike_price
                                    {
                                        output_tx.send(
                                            MarketStreamEvent::Resolved {
                                                final_price: strike_price
                                            }
                                        ).unwrap();
                                    }

                                }
                            },
                            MarketPollState::Resolved => {},
                        }
                    },
                    web_socket_message = async {
                        match web_socket.as_mut() {
                            Some(ws) => ws.next().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        match web_socket_message {
                            Some(Ok(message)) => {

                                // println!("{}", message);

                                // output_tx.send(message)
                            }
                            Some(Err(e)) => {
                                let error_string = e.to_string();

                                println!("{error_string}");
                                let _ = output_tx.send(
                                    MarketStreamEvent::FatalNetworkError(e.to_string())
                                );
                                web_socket = None;
                            }
                            None => {
                                web_socket = None;
                            }
                        }
                    },
                }
            }
        });

        KalshiMarketReader {
            input_tx,
            output_rx,
            state_cache: initial_state
        }
    }

    pub fn set_poll_state(&mut self, new_state: MarketPollState)
    {
        self.state_cache = new_state.clone();
        self.input_tx.send(new_state).unwrap();
    }

    pub fn get_poll_state(&self) -> MarketPollState
    {
        self.state_cache.clone()
    }

    // pub fn subscribe
    pub fn get_receiver(&mut self) -> &mut UnboundedReceiver<MarketStreamEvent>
    {
        &mut self.output_rx
    }
}
