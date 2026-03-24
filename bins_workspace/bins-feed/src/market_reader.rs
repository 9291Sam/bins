const TRADE_API_SOCKET: &str = "wss://api.elections.kalshi.com/trade-api/ws/v2";

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use bins_core::Orderbook;
use chrono::Utc;
use futures_util::StreamExt;
use futures_util::sink::SinkExt;
use reqwest::Client;
use rsa::RsaPrivateKey;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pss::BlindedSigningKey;
use rsa::rand_core::OsRng;
use rsa::signature::{RandomizedSigner, SignatureEncoding};
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::interval;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::{KalshiMarketDescriptor, MarketTicker};

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
        priv_key_path: String,
        on_update: Arc<dyn Fn() + Send + Sync>
    ) -> KalshiMarketReader
    {
        let (input_tx, mut input_rx) = unbounded_channel();
        let (mut output_tx, output_rx) = unbounded_channel();
        let task_state = initial_state.clone();

        tokio::task::spawn(async move {
            let ticker = ticker;
            let api_key_id = api_key_id;
            let priv_key_path = priv_key_path;
            let mut state = task_state;
            let on_update = on_update.clone();

            const TIME_BETWEEN_RESOLVE_POLLS_MS: u64 = 1000;
            const TIME_BETWEEN_TICKS_MS: u64 = 25;
            const TICKS_BETWEEN_RESOLVE_POLLS: u64 =
                TIME_BETWEEN_RESOLVE_POLLS_MS / TIME_BETWEEN_TICKS_MS;

            let mut interval = interval(Duration::from_millis(TIME_BETWEEN_TICKS_MS));
            let mut ticks_since_last_resolve_poll = 0;

            let rest_client: Client = Client::new();
            let mut web_socket: Option<WebSocketStream<MaybeTlsStream<TcpStream>>> = None;

            fn handle_incoming_websocket_message(
                message: String,
                output_tx: &mut UnboundedSender<MarketStreamEvent>,
                on_update: &Arc<dyn Fn() + Send + Sync>
            )
            {
                if let Ok(ws_msg) = serde_json::from_str::<KalshiWsMessage>(&message)
                {
                    match ws_msg
                    {
                        KalshiWsMessage::OrderbookSnapshot {
                            msg, ..
                        } =>
                        {
                            let mut snapshot = Orderbook::new();
                            for [price_str, size_str] in msg.yes_dollars_fp
                            {
                                if let (Ok(price), Ok(size)) =
                                    (price_str.parse::<f64>(), size_str.parse::<f64>())
                                {
                                    snapshot.set_shares(price, size as i32);
                                }
                            }
                            for [price_str, size_str] in msg.no_dollars_fp
                            {
                                if let (Ok(price), Ok(size)) =
                                    (price_str.parse::<f64>(), size_str.parse::<f64>())
                                {
                                    snapshot.set_shares(1.0 - price, -size as i32);
                                }
                            }
                            let _ = output_tx.send(MarketStreamEvent::OrderbookSnapshot(snapshot));
                            on_update();
                        }
                        KalshiWsMessage::OrderbookDelta {
                            msg:
                                DeltaMsg {
                                    price_dollars,
                                    delta_fp,
                                    side
                                },
                            ..
                        } =>
                        {
                            let (aligned_price, size_delta) = if side == KalshiMarketSide::Yes
                            {
                                (price_dollars, delta_fp as i32)
                            }
                            else
                            {
                                (1.0 - price_dollars, -(delta_fp as i32))
                            };
                            let _ = output_tx.send(MarketStreamEvent::OrderbookDelta {
                                price_dollars: aligned_price,
                                size_delta
                            });
                            on_update();
                        }
                        _ =>
                        {}
                    }
                }
            }

            loop
            {
                tokio::select! {
                    v = input_rx.recv() => {
                        match v {
                            Some(new_state) => state = new_state,
                            None => break
                        }
                    },
                    _ = interval.tick() => {
                        let mut connect_to_websocket = async || {
                            if web_socket.is_none() {
                                web_socket = Some(
                                    connect_ws(
                                        &[ticker.0.clone()],
                                        &api_key_id,
                                        &priv_key_path
                                    ).await.unwrap()
                                );
                            }
                        };

                        let mut poll_if_not_recent = async || {
                            ticks_since_last_resolve_poll += 1;
                            if ticks_since_last_resolve_poll > TICKS_BETWEEN_RESOLVE_POLLS {
                                ticks_since_last_resolve_poll = 0;
                                let now = Utc::now();
                                let nearby_markets = super::poll_nearby_markets(
                                    &rest_client, now
                                ).await;
                                if output_tx.send(
                                    MarketStreamEvent::NewDescriptors(nearby_markets)
                                ).is_ok() {
                                    on_update();
                                }
                            }
                        };

                        match state {
                            MarketPollState::FarBeforeActive => {},
                            MarketPollState::RightBeforeActive |
                            MarketPollState::ActiveLookingForStrike |
                            MarketPollState::ActiveKnownStrike => {
                                connect_to_websocket().await;
                                if state == MarketPollState::ActiveLookingForStrike {
                                    poll_if_not_recent().await;
                                }
                            },
                            MarketPollState::ActivelyTryingToResolve => {
                                poll_if_not_recent().await;
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
                                handle_incoming_websocket_message(
                                    message.to_string(),
                                    &mut output_tx,
                                    &on_update
                                );
                            }
                            Some(Err(e)) => panic!("{}", e.to_string()),
                            None => web_socket = None,
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

    pub fn get_receiver(&mut self) -> &mut UnboundedReceiver<MarketStreamEvent>
    {
        &mut self.output_rx
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum KalshiWsMessage
{
    #[serde(rename = "orderbook_snapshot")]
    OrderbookSnapshot
    {
        msg: SnapshotMsg
    },
    #[serde(rename = "orderbook_delta")]
    OrderbookDelta
    {
        msg: DeltaMsg
    },
    #[serde(other)]
    Unknown
}

#[derive(Debug, Deserialize)]
struct SnapshotMsg
{
    #[serde(default)]
    pub yes_dollars_fp: Vec<[String; 2]>,
    #[serde(default)]
    pub no_dollars_fp:  Vec<[String; 2]>
}

#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
enum KalshiMarketSide
{
    #[serde(rename = "yes")]
    Yes,
    #[serde(rename = "no")]
    No
}

#[derive(Debug, Deserialize)]
struct DeltaMsg
{
    #[serde(default, deserialize_with = "super::deserialize_stringified_float")]
    pub price_dollars: f64,
    #[serde(default, deserialize_with = "super::deserialize_stringified_float")]
    pub delta_fp:      f64,
    pub side:          KalshiMarketSide
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MarketPollState
{
    FarBeforeActive,
    RightBeforeActive,
    ActiveLookingForStrike,
    ActiveKnownStrike,
    ActivelyTryingToResolve,
    Resolved
}

#[allow(clippy::large_enum_variant)]
pub enum MarketStreamEvent
{
    OrderbookSnapshot(Orderbook),
    OrderbookDelta
    {
        price_dollars: f64,
        size_delta:    i32
    },
    NewDescriptors(Vec<KalshiMarketDescriptor>)
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
        }})
        .to_string()
        .into()
    ))
    .await?;
    Ok(ws)
}
