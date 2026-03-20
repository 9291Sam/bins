use std::env;
use std::io::Stdout;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use crossterm::event::{Event, EventStream, KeyCode};
use crossterm::terminal::{
    EnterAlternateScreen,
    LeaveAlternateScreen,
    disable_raw_mode,
    enable_raw_mode
};
use futures_util::StreamExt;
use kalshi::{
    BitcoinPriceGrabber,
    BitcoinPriceUpdate,
    MarketPollState,
    MarketStreamEvent,
    PreviousCurrentAndNextMarkets,
    poll_previous_current_and_next_market
};
use ratatui::Terminal;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::CrosstermBackend;
use reqwest::Client;
use tokio::time::interval;

use crate::kalshi::KalshiMarketStatus;
use crate::renderer::{MarketRenderData, render_market};
use crate::shared::MarketBundle;

mod kalshi;
mod renderer;
mod shared;

#[tokio::main]
async fn main() -> std::io::Result<()>
{
    dotenvy::dotenv().ok();

    let api_key_id = env::var("KALSHI_API_KEY_ID").expect("Missing KALSHI_API_KEY_ID");
    let priv_key_path =
        env::var("KALSHI_PRIVATE_KEY_PATH").expect("Missing KALSHI_PRIVATE_KEY_PATH");

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let mut terminal: Terminal<CrosstermBackend<Stdout>> =
        ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;

    let (mut next, mut current, mut previous) = {
        let PreviousCurrentAndNextMarkets {
            next_market,
            current_market,
            previous_market
        } = poll_previous_current_and_next_market(&Client::new(), Utc::now()).await;

        let next = MarketBundle::new(
            next_market,
            MarketPollState::FarBeforeActive,
            api_key_id.clone(),
            priv_key_path.clone()
        );
        let current = MarketBundle::new(
            current_market,
            MarketPollState::Active,
            api_key_id.clone(),
            priv_key_path.clone()
        );
        let previous = MarketBundle::new(
            previous_market,
            MarketPollState::ActivelyTryingToResolve,
            api_key_id.clone(),
            priv_key_path.clone()
        );

        (next, current, previous)
    };

    const TIME_BETWEEN_UPDATE_TICKS_MS: u64 = 25;

    let mut interval = interval(Duration::from_millis(TIME_BETWEEN_UPDATE_TICKS_MS));

    async fn tick_update_function(
        next: &mut MarketBundle,
        current: &mut MarketBundle,
        previous: &mut MarketBundle,
        api_key_id: &str,
        priv_key_path: &str,
        now: DateTime<Utc>
    )
    {
        if next.communicator.get_poll_state() == MarketPollState::FarBeforeActive
            && now - next.get_start_time() < TimeDelta::seconds(30)
        {
            next.communicator
                .set_poll_state(MarketPollState::RightBeforeActive);
        }

        if (now - current.close_time) > TimeDelta::zero()
        {
            let new_next_descriptor = poll_previous_current_and_next_market(&Client::new(), now)
                .await
                .next_market;

            let new_next = MarketBundle::new(
                new_next_descriptor,
                MarketPollState::FarBeforeActive,
                api_key_id.to_owned(),
                priv_key_path.to_owned()
            );

            // current -> previous
            std::mem::swap(previous, current);

            // next -> current
            std::mem::swap(next, current);

            // new_next -> next && drop old previous
            std::mem::drop(std::mem::replace(next, new_next));

            current.communicator.set_poll_state(MarketPollState::Active);
            previous
                .communicator
                .set_poll_state(MarketPollState::ActivelyTryingToResolve);
        }

        if previous.communicator.get_poll_state() == MarketPollState::ActivelyTryingToResolve
        {
            if previous.status == KalshiMarketStatus::Determined
                || previous.status == KalshiMarketStatus::Finalized
            {
                // TODO: make these statuses more fine grained
                previous
                    .communicator
                    .set_poll_state(MarketPollState::Resolved);
            }
        }
    }

    fn create_render_data_from_bundle(
        bundle: &'_ MarketBundle,
        now: DateTime<Utc>,
        real_bitcoin_price: f64,
        approximated_bitcoin_price: f64
    ) -> MarketRenderData<'_>
    {
        let state = bundle.communicator.get_poll_state();

        match state
        {
            MarketPollState::Active
            | MarketPollState::RightBeforeActive
            | MarketPollState::FarBeforeActive =>
            {
                MarketRenderData::Active {
                    strike_price: bundle.strike_price,
                    current_bitcoin_price: real_bitcoin_price,
                    market_id: bundle.ticker.0.clone(),
                    time_untill_expiry: bundle.close_time - now,
                    orderbook: bundle.orderbook.clone(),
                    delta_history: &bundle.delta_history,
                    approximated_bitcoin_price
                }
            }
            MarketPollState::ActivelyTryingToResolve =>
            {
                MarketRenderData::Resolving {
                    strike_price:      bundle.strike_price,
                    market_id:         bundle.ticker.0.clone(),
                    time_after_expiry: now - bundle.close_time,
                    orderbook:         bundle.orderbook.clone(),
                    delta_history:     &bundle.delta_history
                }
            }
            MarketPollState::Resolved =>
            {
                MarketRenderData::Resolved {
                    strike_price:        bundle.strike_price.unwrap(),
                    final_bitcoin_price: bundle.final_price.unwrap(),
                    market_id:           bundle.ticker.0.clone(),
                    delta_history:       &bundle.delta_history
                }
            }
        }
    }

    async fn tick_render_function(
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        current: &mut MarketBundle,
        previous: &mut MarketBundle,
        now: DateTime<Utc>,
        real_bitcoin_price: f64,
        approximated_bitcoin_price: f64
    )
    {
        terminal
            .draw(|frame| {
                let columns = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Fill(1), Constraint::Fill(1)])
                    .split(frame.area());

                render_market(
                    frame,
                    columns[0],
                    &create_render_data_from_bundle(
                        current,
                        now,
                        real_bitcoin_price,
                        approximated_bitcoin_price
                    )
                );
                render_market(
                    frame,
                    columns[1],
                    &create_render_data_from_bundle(
                        previous,
                        now,
                        real_bitcoin_price,
                        approximated_bitcoin_price
                    )
                );
            })
            .unwrap();
    }

    let mut crossterm_events = EventStream::new();
    let mut real_bitcoin_price: f64 = 0.0;
    let mut approximated_bitcoin_price: f64 = 0.0;

    let mut bitcoin_price_grabber = BitcoinPriceGrabber::new();

    loop
    {
        tokio::select! {
            _ = interval.tick() => {
                let now = Utc::now();

                tick_update_function(
                    &mut next,
                    &mut current,
                    &mut previous,
                    &api_key_id,
                    &priv_key_path,
                    now,
                ).await;

                tick_render_function(
                    &mut terminal,
                    &mut current,
                    &mut previous,
                    now,
                    real_bitcoin_price,
                    approximated_bitcoin_price
                ).await;

            },
            Some(Ok(Event::Key(key))) = crossterm_events.next() => {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    break;
                }
            },
            Some(e) = next.communicator.get_receiver().recv() => {
                next.apply_event(e);
            }
            Some(e) = current.communicator.get_receiver().recv() => {
                current.apply_event(e);
            }
            Some(e) = previous.communicator.get_receiver().recv() => {

                previous.apply_event(e);
            }
            Some(u) = bitcoin_price_grabber.get_receiver().recv() => {
                match u
                {
                    BitcoinPriceUpdate::Official(o) => real_bitcoin_price = o,
                    BitcoinPriceUpdate::Approximated(a) => approximated_bitcoin_price = a,
                }
            }

        }
    }

    disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
