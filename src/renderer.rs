use std::borrow::Cow;

use egui::{Color32, Grid, RichText, Ui};
use egui_plot::{HLine, Line, Plot, PlotPoints};

use crate::shared::{
    DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE,
    DeltaHistory,
    MARKET_INTERVAL_MINUTES,
    MARKET_INTERVAL_SECONDS,
    Orderbook,
    index_to_dollars
};

pub enum MarketRenderData<'a>
{
    Active
    {
        strike_price:               Option<f64>,
        current_bitcoin_price:      f64,
        approximated_bitcoin_price: f64,
        market_id:                  String,
        time_untill_expiry:         chrono::Duration,
        orderbook:                  Orderbook,
        delta_history:              &'a DeltaHistory
    },
    Resolving
    {
        strike_price:      Option<f64>,
        market_id:         String,
        time_after_expiry: chrono::Duration,
        orderbook:         Orderbook,
        delta_history:     &'a DeltaHistory
    },
    Resolved
    {
        strike_price:        f64,
        final_bitcoin_price: f64,
        market_id:           String,
        delta_history:       &'a DeltaHistory
    }
}

impl<'a> MarketRenderData<'a>
{
    pub fn get_strike_price(&self) -> Option<f64>
    {
        match self
        {
            MarketRenderData::Active {
                strike_price, ..
            }
            | MarketRenderData::Resolving {
                strike_price, ..
            } => *strike_price,
            MarketRenderData::Resolved {
                strike_price, ..
            } => Some(*strike_price)
        }
    }

    pub fn get_market_id(&self) -> &String
    {
        match self
        {
            MarketRenderData::Active {
                market_id, ..
            }
            | MarketRenderData::Resolving {
                market_id, ..
            }
            | MarketRenderData::Resolved {
                market_id, ..
            } => market_id
        }
    }

    pub fn get_delta_history(&'a self) -> &'a DeltaHistory
    {
        match self
        {
            MarketRenderData::Active {
                delta_history, ..
            }
            | MarketRenderData::Resolving {
                delta_history, ..
            }
            | MarketRenderData::Resolved {
                delta_history, ..
            } => delta_history
        }
    }
}

pub fn render_market(ui: &mut Ui, data: &MarketRenderData)
{
    match data
    {
        MarketRenderData::Active {
            ..
        }
        | MarketRenderData::Resolving {
            ..
        } =>
        {
            ui.vertical(|ui| {
                render_header(ui, data);
                ui.add_space(8.0);
                render_orderbook(ui, data);
                ui.add_space(8.0);
                render_chart(ui, data);
            });
        }
        MarketRenderData::Resolved {
            ..
        } =>
        {
            ui.vertical(|ui| {
                render_header(ui, data);
                ui.add_space(8.0);
                render_chart(ui, data);
            });
        }
    }
}

fn render_header(ui: &mut Ui, data: &MarketRenderData)
{
    ui.group(|ui| {
        ui.set_width(ui.available_width());

        // Terminal Title
        ui.label(
            RichText::new(format!(
                "Kalshi {} minute terminal | {}",
                MARKET_INTERVAL_MINUTES,
                data.get_market_id()
            ))
            .color(Color32::from_rgb(255, 0, 255)) // Magenta
            .strong()
        );

        ui.add_space(4.0);

        // Prices & Delta
        ui.horizontal(|ui| {
            let strike_price_string = format!(
                "Strike: {}",
                data.get_strike_price()
                    .map(|p| Cow::Owned(p.to_string()))
                    .unwrap_or(Cow::Borrowed("---"))
            );
            ui.label(RichText::new(strike_price_string).color(Color32::LIGHT_BLUE));

            match data
            {
                MarketRenderData::Active {
                    current_bitcoin_price,
                    approximated_bitcoin_price,
                    ..
                } =>
                {
                    ui.label(
                        RichText::new(format!(
                            "| Live Bitcoin Price: ${current_bitcoin_price:.2} | Estimate: \
                             ${approximated_bitcoin_price:.2}"
                        ))
                        .color(Color32::YELLOW)
                    );
                }
                MarketRenderData::Resolved {
                    final_bitcoin_price,
                    ..
                } =>
                {
                    ui.label(
                        RichText::new(format!("| Final Bitcoin Price: ${final_bitcoin_price}"))
                            .color(Color32::YELLOW)
                    );
                }
                _ =>
                {}
            }

            // Delta Calculation
            let delta: Option<f64> = match data
            {
                MarketRenderData::Active {
                    strike_price: Some(strike_price),
                    current_bitcoin_price,
                    ..
                } => Some(current_bitcoin_price - strike_price),
                MarketRenderData::Resolved {
                    strike_price,
                    final_bitcoin_price,
                    ..
                } => Some(final_bitcoin_price - strike_price),
                _ => None
            };

            if let Some(delta) = delta
            {
                let color = if delta.is_sign_positive()
                {
                    Color32::GREEN
                }
                else
                {
                    Color32::RED
                };
                ui.label(RichText::new(format!("| Delta: {:+.2}", delta)).color(color));
            }
        });

        ui.add_space(4.0);

        // Expiry Status
        match data
        {
            MarketRenderData::Active {
                time_untill_expiry, ..
            } =>
            {
                let secs = time_untill_expiry.as_seconds_f64();
                let color = if secs > 300.0
                {
                    Color32::WHITE
                }
                else if secs > 60.0
                {
                    Color32::YELLOW
                }
                else
                {
                    Color32::RED
                };

                ui.label(
                    RichText::new(format!(
                        "Market Expiring in {}.{}s",
                        time_untill_expiry.num_seconds(),
                        time_untill_expiry.num_milliseconds() % 1000
                    ))
                    .color(color)
                );
            }
            MarketRenderData::Resolving {
                time_after_expiry, ..
            } =>
            {
                ui.label(
                    RichText::new(format!(
                        "Market Resolving {}.{}s elapsed",
                        time_after_expiry.num_seconds(),
                        time_after_expiry.num_milliseconds() % 1000
                    ))
                    .color(Color32::WHITE)
                );
            }
            MarketRenderData::Resolved {
                strike_price,
                final_bitcoin_price,
                ..
            } =>
            {
                let color = if (strike_price - final_bitcoin_price) > 0.0
                {
                    Color32::RED
                }
                else
                {
                    Color32::GREEN
                };
                ui.label(RichText::new("Market Resolved").color(color));
            }
        }
    });
}
fn render_orderbook(ui: &mut Ui, data: &MarketRenderData)
{
    let orderbook = match data
    {
        MarketRenderData::Active {
            orderbook, ..
        } => orderbook,
        MarketRenderData::Resolving {
            orderbook, ..
        } => orderbook,
        MarketRenderData::Resolved {
            ..
        } => return
    };

    // Asks
    let mut asks: Vec<(f64, i32)> = orderbook
        .data
        .iter()
        .enumerate()
        .filter_map(|(idx, &shares)| {
            if shares < 0
            {
                Some((index_to_dollars(idx).unwrap_or(0.0) * 100.0, shares.abs()))
            }
            else
            {
                None
            }
        })
        .take(8)
        .collect();
    asks.reverse();

    // Bids
    let bids: Vec<(f64, i32)> = orderbook
        .data
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(idx, &shares)| {
            if shares > 0
            {
                Some((index_to_dollars(idx).unwrap_or(0.0) * 100.0, shares))
            }
            else
            {
                None
            }
        })
        .take(8)
        .collect();

    let best_ask = asks.last().map(|(cents, _)| *cents);
    let best_bid = bids.first().map(|(cents, _)| *cents);
    let spread_s = match (best_ask, best_bid)
    {
        (Some(ask), Some(bid)) if ask - bid > 0.0 => format!("{:>4.1}¢", ask - bid),
        _ => " - ".to_string()
    };

    ui.group(|ui| {
        // FIX: Inject the unique market ID into the Grid identifier
        Grid::new(format!("orderbook_grid_{}", data.get_market_id()))
            .striped(true)
            .min_col_width(80.0)
            .show(ui, |ui| {
                // Header
                ui.label(RichText::new("BUY / SELL").color(Color32::DARK_GRAY));
                ui.label("");
                ui.end_row();

                ui.label(RichText::new("YES ¢ / NO ¢").color(Color32::DARK_GRAY));
                ui.label(RichText::new("SIZE").color(Color32::DARK_GRAY));
                ui.end_row();

                // Asks (Red)
                for _ in 0..8usize.saturating_sub(asks.len())
                {
                    ui.label(RichText::new("-").color(Color32::DARK_GRAY));
                    ui.label(RichText::new("-").color(Color32::DARK_GRAY));
                    ui.end_row();
                }
                for (cents, shares) in &asks
                {
                    ui.label(
                        RichText::new(format!("{:>5.1} / {:<5.1}", cents, 100.0 - cents))
                            .color(Color32::LIGHT_RED)
                    );
                    ui.label(RichText::new(format!("{}", shares)).color(Color32::LIGHT_RED));
                    ui.end_row();
                }

                // Spread (Yellow)
                ui.label(
                    RichText::new(format!("SPREAD: {}", spread_s))
                        .color(Color32::YELLOW)
                        .strong()
                );
                ui.label("");
                ui.end_row();

                // Bids (Green)
                for (cents, shares) in &bids
                {
                    ui.label(
                        RichText::new(format!("{:>5.1} / {:<5.1}", cents, 100.0 - cents))
                            .color(Color32::LIGHT_GREEN)
                    );
                    ui.label(RichText::new(format!("{}", shares)).color(Color32::LIGHT_GREEN));
                    ui.end_row();
                }
                for _ in 0..8usize.saturating_sub(bids.len())
                {
                    ui.label(RichText::new("-").color(Color32::DARK_GRAY));
                    ui.label(RichText::new("-").color(Color32::DARK_GRAY));
                    ui.end_row();
                }

                // Footer
                ui.label(RichText::new("YES ¢ / NO ¢").color(Color32::DARK_GRAY));
                ui.label(RichText::new("SIZE").color(Color32::DARK_GRAY));
                ui.end_row();

                ui.label(RichText::new("SELL / BUY").color(Color32::DARK_GRAY));
                ui.label("");
                ui.end_row();
            });
    });
}

fn render_chart(ui: &mut Ui, data: &MarketRenderData)
{
    let mut min_delta: Option<f64> = None;
    let mut max_delta: Option<f64> = None;

    let points: PlotPoints = (0..DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE)
        .filter_map(|idx| {
            let delta = data.get_delta_history()[idx];
            if delta == 0.0
            {
                return None;
            }

            min_delta = Some(min_delta.unwrap_or(delta).min(delta));
            max_delta = Some(max_delta.unwrap_or(delta).max(delta));

            // Map the array index range to a 0 to 900 seconds scale
            let time_seconds = (idx as f64
                / (DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE
                    .saturating_sub(1)
                    .max(1)) as f64)
                * MARKET_INTERVAL_SECONDS as f64;

            Some([time_seconds, delta])
        })
        .collect();

    let min_d = min_delta.unwrap_or(-1.0).min(-1.0);
    let max_d = max_delta.unwrap_or(1.0).max(1.0);

    let line = Line::new("line", points).color(Color32::CYAN).width(2.0);

    Plot::new(format!("delta_history_plot_{}", data.get_market_id()))
        .height(ui.available_height())
        .show_axes([true, true])
        .show_grid([true, true])
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .set_margin_fraction(egui::Vec2::new(0.0, 0.05))
        .include_x(0.0)
        .include_x(900.0)
        .include_y(min_d)
        .include_y(max_d)
        .show(ui, |plot_ui| {
            plot_ui.hline(HLine::new("hline", 0.0).color(Color32::YELLOW).width(1.5));
            plot_ui.line(line);
        });
}
