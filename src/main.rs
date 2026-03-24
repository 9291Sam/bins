use std::env;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use eframe::egui;
use kalshi::{
    BitcoinPriceGrabber,
    BitcoinPriceUpdate,
    MarketPollState,
    PreviousCurrentAndNextMarkets,
    poll_previous_current_and_next_market
};
use meth::Meth;
use reqwest::Client;

use crate::kalshi::KalshiMarketStatus;
use crate::renderer::{MarketRenderData, render_market};
use crate::shared::{MarketArchive, MarketBundle, MarketTick};

mod kalshi;
mod renderer;
mod shared;

struct KalshiApp
{
    rt:                         tokio::runtime::Runtime,
    next:                       MarketBundle,
    current:                    MarketBundle,
    previous:                   MarketBundle,
    bitcoin_price_grabber:      BitcoinPriceGrabber,
    real_bitcoin_price:         f64,
    approximated_bitcoin_price: f64,
    api_key_id:                 String,
    priv_key_path:              String,

    market_fetch_rx: Option<tokio::sync::oneshot::Receiver<PreviousCurrentAndNextMarkets>>
}

fn create_render_data_from_bundle<'a>(
    bundle: &'a MarketBundle,
    now: DateTime<Utc>,
    real_bitcoin_price: f64,
    approximated_bitcoin_price: f64
) -> MarketRenderData<'a>
{
    let state = bundle.communicator.get_poll_state();
    let start_time = bundle.get_start_time();

    match state
    {
        MarketPollState::ActiveLookingForStrike
        | MarketPollState::ActiveKnownStrike
        | MarketPollState::RightBeforeActive
        | MarketPollState::FarBeforeActive =>
        {
            MarketRenderData::Active {
                strike_price: bundle.strike_price,
                current_bitcoin_price: real_bitcoin_price,
                market_id: bundle.ticker.0.clone(),
                time_untill_expiry: bundle.close_time - now,
                orderbook: bundle.orderbook.clone(),
                tick_history: &bundle.tick_history,
                approximated_bitcoin_price,
                start_time
            }
        }
        MarketPollState::ActivelyTryingToResolve =>
        {
            MarketRenderData::Resolving {
                strike_price: bundle.strike_price,
                market_id: bundle.ticker.0.clone(),
                time_after_expiry: now - bundle.close_time,
                orderbook: bundle.orderbook.clone(),
                tick_history: &bundle.tick_history,
                start_time
            }
        }
        MarketPollState::Resolved =>
        {
            MarketRenderData::Resolved {
                strike_price: bundle.strike_price.unwrap_or(0.0),
                final_bitcoin_price: bundle.final_price.unwrap_or(0.0),
                market_id: bundle.ticker.0.clone(),
                tick_history: &bundle.tick_history,
                start_time
            }
        }
    }
}

impl eframe::App for KalshiApp
{
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame)
    {
        let now = Utc::now();
        let mut current_state_changed = false;

        // 1. Resolve pending async market fetches
        let mut fetch_ready = false;
        let mut new_markets = None;
        if let Some(rx) = &mut self.market_fetch_rx
        {
            match rx.try_recv()
            {
                Ok(markets) =>
                {
                    fetch_ready = true;
                    new_markets = Some(markets);
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) =>
                {
                    fetch_ready = true;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) =>
                {}
            }
        }

        if fetch_ready
        {
            self.market_fetch_rx = None;
            if let Some(markets) = new_markets
            {
                let _guard = self.rt.enter();

                let new_next = MarketBundle::new(
                    markets.next_market,
                    MarketPollState::FarBeforeActive,
                    self.api_key_id.clone(),
                    self.priv_key_path.clone(),
                    ctx.clone()
                );

                std::mem::swap(&mut self.previous, &mut self.current);
                std::mem::swap(&mut self.next, &mut self.current);
                self.next = new_next;

                self.current
                    .communicator
                    .set_poll_state(MarketPollState::ActiveLookingForStrike);
                self.previous
                    .communicator
                    .set_poll_state(MarketPollState::ActivelyTryingToResolve);
            }
        }

        // 2. Poll all non-blocking channels
        while let Ok(e) = self.next.communicator.get_receiver().try_recv()
        {
            self.next.apply_event(e);
        }
        while let Ok(e) = self.current.communicator.get_receiver().try_recv()
        {
            self.current.apply_event(e);
            current_state_changed = true;
        }
        while let Ok(e) = self.previous.communicator.get_receiver().try_recv()
        {
            self.previous.apply_event(e);
        }
        while let Ok(u) = self.bitcoin_price_grabber.get_receiver().try_recv()
        {
            match u
            {
                BitcoinPriceUpdate::Official(o) => self.real_bitcoin_price = o,
                BitcoinPriceUpdate::Approximated(a) => self.approximated_bitcoin_price = a
            }
            current_state_changed = true;
        }

        // 3. Tick state updates
        if self.next.communicator.get_poll_state() == MarketPollState::FarBeforeActive
            && now - self.next.get_start_time() < TimeDelta::seconds(30)
        {
            self.next
                .communicator
                .set_poll_state(MarketPollState::RightBeforeActive);
        }

        if (now - self.current.close_time) > TimeDelta::zero() && self.market_fetch_rx.is_none()
        {
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.rt.spawn(async move {
                let res = poll_previous_current_and_next_market(&Client::new(), now).await;
                let _ = tx.send(res);
            });
            self.market_fetch_rx = Some(rx);
        }

        // 3.5 Archiving: Save to disk when previous market finalizes
        if self.previous.communicator.get_poll_state() == MarketPollState::ActivelyTryingToResolve
            && (self.previous.status == KalshiMarketStatus::Determined
                || self.previous.status == KalshiMarketStatus::Finalized)
        {
            self.previous
                .communicator
                .set_poll_state(MarketPollState::Resolved);

            if let Err(e) = MarketArchive::save_to_disk(&self.previous, "./market_data")
            {
                eprintln!(
                    "Failed to save market archive for {}: {:?}",
                    self.previous.ticker.0, e
                );
            }
            else
            {
                println!(
                    "Successfully archived market {} to disk.",
                    self.previous.ticker.0
                );
            }
        }

        if self.current.communicator.get_poll_state() == MarketPollState::ActiveLookingForStrike
            && self.current.strike_price.is_some()
        {
            self.current
                .communicator
                .set_poll_state(MarketPollState::ActiveKnownStrike);
        }

        // 3.6 Sparse Logging: Only record tick if a state physically updated
        if current_state_changed
        {
            let off_price = if self.real_bitcoin_price > 0.0
            {
                Some(self.real_bitcoin_price)
            }
            else
            {
                None
            };
            let app_price = if self.approximated_bitcoin_price > 0.0
            {
                Some(self.approximated_bitcoin_price)
            }
            else
            {
                None
            };

            self.current.tick_history.push(MarketTick {
                timestamp_ms:           now.timestamp_millis(),
                official_bitcoin_price: off_price,
                approx_bitcoin_price:   app_price,
                market_mid_cents:       self.current.orderbook.get_mid_cents(),
                orderbook:              self.current.orderbook.clone()
            });
        }

        // 4. Render UI
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |columns| {
                render_market(
                    &mut columns[0],
                    &create_render_data_from_bundle(
                        &self.current,
                        now,
                        self.real_bitcoin_price,
                        self.approximated_bitcoin_price
                    )
                );
                render_market(
                    &mut columns[1],
                    &create_render_data_from_bundle(
                        &self.previous,
                        now,
                        self.real_bitcoin_price,
                        self.approximated_bitcoin_price
                    )
                );
            });
        });

        // 5. Force the loop to continue at a low heartbeat for timers
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

fn main() -> eframe::Result<()>
{
    let _meth = Meth::new();

    dotenvy::dotenv().ok();

    let api_key_id = env::var("KALSHI_API_KEY_ID").expect("Missing KALSHI_API_KEY_ID");
    let priv_key_path =
        env::var("KALSHI_PRIVATE_KEY_PATH").expect("Missing KALSHI_PRIVATE_KEY_PATH");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to construct tokio runtime");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Kalshi Terminal",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let _guard = rt.enter();

            let (next, current, previous) = rt.block_on(async {
                let PreviousCurrentAndNextMarkets {
                    next_market,
                    current_market,
                    previous_market
                } = poll_previous_current_and_next_market(&Client::new(), Utc::now()).await;

                let next = MarketBundle::new(
                    next_market,
                    MarketPollState::FarBeforeActive,
                    api_key_id.clone(),
                    priv_key_path.clone(),
                    ctx.clone()
                );
                let current = MarketBundle::new(
                    current_market,
                    MarketPollState::ActiveLookingForStrike,
                    api_key_id.clone(),
                    priv_key_path.clone(),
                    ctx.clone()
                );
                let previous = MarketBundle::new(
                    previous_market,
                    MarketPollState::ActivelyTryingToResolve,
                    api_key_id.clone(),
                    priv_key_path.clone(),
                    ctx.clone()
                );

                (next, current, previous)
            });

            let bitcoin_price_grabber = BitcoinPriceGrabber::new(ctx.clone());

            Ok(Box::new(KalshiApp {
                rt,
                next,
                current,
                previous,
                bitcoin_price_grabber,
                real_bitcoin_price: 0.0,
                approximated_bitcoin_price: 0.0,
                api_key_id,
                priv_key_path,
                market_fetch_rx: None
            }))
        })
    )
}
