use chrono::Utc;
use crossterm::terminal::{
    EnterAlternateScreen,
    LeaveAlternateScreen,
    disable_raw_mode,
    enable_raw_mode
};
use reqwest::Client;

use crate::kalshi_communicator::poll_nearby_markets;
use crate::renderer::render_market;

mod kalshi_communicator;
mod renderer;

pub const MARKET_INTERVAL_MINUTES: usize = 15;
pub const MARKET_INTERVAL_SECONDS: usize = MARKET_INTERVAL_MINUTES * 60;

pub const SCREEN_UPDATES_HZ: usize = 60;
pub const SAVING_INTERVAL_HZ: usize = 4;

pub const DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE: usize =
    SAVING_INTERVAL_HZ * MARKET_INTERVAL_SECONDS;

pub type OrderBookShares = [i32; 100];
pub type CompleteOrderBookRecord = [OrderBookShares; DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE];

pub type DeltaHistory = [f64; DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE];

#[tokio::main]
async fn main() -> std::io::Result<()>
{
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;

    let data = renderer::MarketRenderData::Active {
        strike_price:          Some(1234.45),
        current_bitcoin_price: 122334.3,
        market_id:             "market id".into(),
        time_untill_expiry:    chrono::Duration::minutes(12),
        orderbook_shares:      [
            -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100,
            -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100,
            -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100, -100,
            -100, -100, -100, -100, -100, -100, -100, -100, 0, 100, 100, 100, 100, 100, 100, 100,
            100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100,
            100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100,
            100, 100, 100, 100, 100, 100, 100, 100
        ],
        delta_history:         std::array::from_fn(|idx| {
            let unit_along = idx as f64 / DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE as f64;

            (unit_along * 12.0).sin()
        })
    };

    // loop
    // {
    //     terminal.draw(|f| render_market(f, f.area(), &data))?;
    // }

    disable_raw_mode()?;
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    println!("Hello, world!");

    let res = poll_nearby_markets(&Client::new(), Utc::now()).await;

    for m in res
    {
        println!("Market: {:?}", m);
    }

    Ok(())
}
