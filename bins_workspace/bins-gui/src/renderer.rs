use std::borrow::Cow;

use bins_core::{MARKET_INTERVAL_MINUTES, MarketTick, Orderbook, index_to_dollars};
use chrono::{DateTime, Utc};
use egui::{Color32, Grid, RichText, Ui};
use egui_plot::{HLine, Line, Plot};

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
        start_time:                 DateTime<Utc>,
        tick_history:               &'a [MarketTick]
    },
    Resolving
    {
        strike_price:      Option<f64>,
        market_id:         String,
        time_after_expiry: chrono::Duration,
        orderbook:         Orderbook,
        start_time:        DateTime<Utc>,
        tick_history:      &'a [MarketTick]
    },
    Resolved
    {
        strike_price:        f64,
        final_bitcoin_price: f64,
        market_id:           String,
        start_time:          DateTime<Utc>,
        tick_history:        &'a [MarketTick]
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

    pub fn get_start_time(&self) -> DateTime<Utc>
    {
        match self
        {
            MarketRenderData::Active {
                start_time, ..
            }
            | MarketRenderData::Resolving {
                start_time, ..
            }
            | MarketRenderData::Resolved {
                start_time, ..
            } => *start_time
        }
    }

    pub fn get_tick_history(&'a self) -> &'a [MarketTick]
    {
        match self
        {
            MarketRenderData::Active {
                tick_history, ..
            }
            | MarketRenderData::Resolving {
                tick_history, ..
            }
            | MarketRenderData::Resolved {
                tick_history, ..
            } => tick_history
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

        ui.label(
            RichText::new(format!(
                "Kalshi {} minute terminal | {}",
                MARKET_INTERVAL_MINUTES,
                data.get_market_id()
            ))
            .color(Color32::from_rgb(255, 0, 255))
            .strong()
        );

        ui.add_space(4.0);

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
        Grid::new(format!("orderbook_grid_{}", data.get_market_id()))
            .striped(true)
            .min_col_width(80.0)
            .show(ui, |ui| {
                ui.label(RichText::new("BUY / SELL").color(Color32::DARK_GRAY));
                ui.label("");
                ui.end_row();

                ui.label(RichText::new("YES ¢ / NO ¢").color(Color32::DARK_GRAY));
                ui.label(RichText::new("SIZE").color(Color32::DARK_GRAY));
                ui.end_row();

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

                ui.label(
                    RichText::new(format!("SPREAD: {}", spread_s))
                        .color(Color32::YELLOW)
                        .strong()
                );
                ui.label("");
                ui.end_row();

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
    let history = data.get_tick_history();
    let strike = data.get_strike_price();
    let start_time_ms = data.get_start_time().timestamp_millis();

    let mut mid_points = vec![];
    let mut official_points = vec![];
    let mut approx_points = vec![];

    let mut min_btc = f64::MAX;
    let mut max_btc = f64::MIN;

    for tick in history.iter()
    {
        let time_seconds = (tick.timestamp_ms - start_time_ms) as f64 / 1000.0;

        if let Some(mid) = tick.market_mid_cents
        {
            mid_points.push([time_seconds, mid]);
        }
        if let Some(off) = tick.official_bitcoin_price
        {
            official_points.push([time_seconds, off]);
            min_btc = min_btc.min(off);
            max_btc = max_btc.max(off);
        }
        if let Some(app) = tick.approx_bitcoin_price
        {
            approx_points.push([time_seconds, app]);
            min_btc = min_btc.min(app);
            max_btc = max_btc.max(app);
        }
    }

    if let Some(s) = strike
    {
        min_btc = min_btc.min(s);
        max_btc = max_btc.max(s);
    }

    if min_btc == f64::MAX
    {
        min_btc = 0.0;
        max_btc = 100000.0;
    }
    else
    {
        let pad = (max_btc - min_btc).max(10.0) * 0.1;
        min_btc -= pad;
        max_btc += pad;
    }

    let available_height = ui.available_height();
    let plot_height = (available_height / 2.0) - 16.0;

    ui.label(
        RichText::new("Market Midpoint (¢)")
            .color(Color32::LIGHT_BLUE)
            .small()
    );
    Plot::new(format!("mid_plot_{}", data.get_market_id()))
        .height(plot_height)
        .show_axes([true, true])
        .show_grid([true, true])
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .set_margin_fraction(egui::Vec2::new(0.0, 0.05))
        .include_x(0.0)
        .include_x(900.0)
        .include_y(0.0)
        .include_y(100.0)
        .show(ui, |plot_ui| {
            plot_ui.line(
                Line::new("mid", mid_points)
                    .color(Color32::LIGHT_BLUE)
                    .width(1.5)
            );
        });

    ui.add_space(8.0);

    ui.label(
        RichText::new("Bitcoin Price ($)")
            .color(Color32::YELLOW)
            .small()
    );
    Plot::new(format!("btc_price_plot_{}", data.get_market_id()))
        .height(ui.available_height())
        .show_axes([true, true])
        .show_grid([true, true])
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .set_margin_fraction(egui::Vec2::new(0.0, 0.05))
        .include_x(0.0)
        .include_x(900.0)
        .include_y(min_btc)
        .include_y(max_btc)
        .show(ui, |plot_ui| {
            if let Some(s) = strike
            {
                plot_ui.hline(HLine::new("strike", s).color(Color32::YELLOW).width(1.5));
            }
            plot_ui.line(
                Line::new("approx", approx_points)
                    .color(Color32::GRAY)
                    .width(1.0)
            );
            plot_ui.line(
                Line::new("official", official_points)
                    .color(Color32::CYAN)
                    .width(2.0)
            );
        });
}
