use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const OFFICIAL_BITCOIN_ENDPOINT: &str =
    "https://kalshi-public-docs.s3.amazonaws.com/external/crypto/btc_current.json";
const UNOFFICIAL_BITCOIN_ENDPOINT: &str = "wss://ws-feed.exchange.coinbase.com";

pub enum BitcoinPriceUpdate
{
    Official(f64),
    Approximated(f64)
}

pub struct BitcoinPriceGrabber
{
    price_updates_rx: UnboundedReceiver<BitcoinPriceUpdate>
}

impl BitcoinPriceGrabber
{
    pub fn new() -> BitcoinPriceGrabber
    {
        let (price_updates_tx, price_updates_rx) = unbounded_channel();

        {
            let price_updates_tx = price_updates_tx.clone();

            tokio::spawn(async move {
                let client = Client::new();

                const MAX_FAILS: u64 = 10;
                let mut fails: u64 = 0;

                loop
                {
                    let response = match client.get(OFFICIAL_BITCOIN_ENDPOINT).send().await
                    {
                        Ok(r) =>
                        {
                            fails = 0;

                            r
                        }
                        Err(e) =>
                        {
                            fails += 1;
                            eprintln!(
                                "Failed to poll bitcoin price, retrying. {fails}/{MAX_FAILS} | \
                                 {:?}",
                                e.status()
                            );

                            if fails > MAX_FAILS
                            {
                                panic!();
                            }

                            tokio::time::sleep(Duration::from_millis((fails + 1) * 2500)).await;

                            continue;
                        }
                    };

                    if let Some(price) = response
                        .json::<Value>()
                        .await
                        .unwrap()
                        .pointer("/timeseries/second")
                        .and_then(|a| a.as_array())
                        .and_then(|a| a.last())
                        .and_then(|v| v.as_f64())
                        && price_updates_tx
                            .send(BitcoinPriceUpdate::Official(price))
                            .is_err()
                    {
                        break;
                    }

                    tokio::time::sleep(Duration::from_millis(1100)).await;
                }
            });
        };

        {
            let price_updates_tx = price_updates_tx.clone();

            tokio::spawn(async move {
                let url = UNOFFICIAL_BITCOIN_ENDPOINT;
                let (ws_stream, _) = connect_async(url)
                    .await
                    .expect("bitcoin websocket failed to connect");

                let (mut write, mut read) = ws_stream.split();

                let sub_msg = json!({
                    "type": "subscribe",
                    "product_ids": ["BTC-USD"],
                    "channels": ["ticker"]
                });

                write
                    .send(Message::Text(sub_msg.to_string().into()))
                    .await
                    .expect("ws subscribe failed");

                while let Some(msg) = read.next().await
                {
                    if let Ok(Message::Text(text)) = msg
                    {
                        let parsed: Value = serde_json::from_str(&text).unwrap_or_default();
                        if parsed["type"] == "ticker"
                            && let Some(price_str) = parsed["price"].as_str()
                            && let Ok(p) = price_str.parse::<f64>()
                            && price_updates_tx
                                .send(BitcoinPriceUpdate::Approximated(p))
                                .is_err()
                        {
                            break;
                        }
                    }
                }
            });
        }

        BitcoinPriceGrabber {
            price_updates_rx
        }
    }

    pub fn get_receiver(&mut self) -> &mut UnboundedReceiver<BitcoinPriceUpdate>
    {
        &mut self.price_updates_rx
    }
}
