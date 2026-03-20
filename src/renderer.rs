use std::borrow::Cow;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::shared::{
    DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE,
    DeltaHistory,
    MARKET_INTERVAL_MINUTES,
    Orderbook,
    index_to_dollars
};

pub enum MarketRenderData<'a>
{
    Active
    {
        strike_price:          Option<f64>,
        current_bitcoin_price: f64,
        market_id:             String,
        time_untill_expiry:    chrono::Duration,
        orderbook:             Orderbook,
        delta_history:         &'a DeltaHistory
    },
    Resolving
    {
        strike_price:                 Option<f64>,
        estimate_final_bitcoin_price: f64,
        market_id:                    String,
        time_after_expiry:            chrono::Duration,
        orderbook:                    Orderbook,
        delta_history:                &'a DeltaHistory
    },
    Resolved
    {
        strike_price:        Option<f64>,
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
            }
            | MarketRenderData::Resolved {
                strike_price, ..
            } => *strike_price
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

pub fn render_market(frame: &mut Frame, area: Rect, data: &MarketRenderData)
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
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(8),
                    Constraint::Length(22),
                    Constraint::Fill(1)
                ])
                .split(area);

            render_header(frame, rows[0], data);

            render_orderbook(frame, rows[1], data);

            render_chart(frame, rows[2], data);
        }
        MarketRenderData::Resolved {
            ..
        } =>
        {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(8), Constraint::Fill(1)])
                .split(area);

            render_header(frame, rows[0], data);

            render_chart(frame, rows[1], data);
        }
    }
}

fn render_header(frame: &mut Frame, area: Rect, data: &MarketRenderData)
{
    let strike_price_string = format!(
        " Strike: {}",
        data.get_strike_price()
            .map(|p| Cow::Owned(p.to_string()))
            .unwrap_or(Cow::Borrowed("---"))
    );

    let bitcoin_price_string = match data
    {
        MarketRenderData::Active {
            current_bitcoin_price,
            ..
        } => format!(" Live Bitcoin Price: ${current_bitcoin_price:.2}"),
        MarketRenderData::Resolving {
            estimate_final_bitcoin_price,
            ..
        } => format!(" Final Bitcoin Estimated Price: ${estimate_final_bitcoin_price}"),
        MarketRenderData::Resolved {
            final_bitcoin_price,
            ..
        } => format!(" Final Bitcoin Price: ${final_bitcoin_price}")
    };

    let (delta_string, delta_color) = {
        let delta: Option<f64> = match data
        {
            MarketRenderData::Active {
                strike_price: Some(strike_price),
                current_bitcoin_price,
                ..
            } => Some(current_bitcoin_price - strike_price),
            MarketRenderData::Resolving {
                strike_price: Some(strike_price),
                estimate_final_bitcoin_price,
                ..
            } => Some(estimate_final_bitcoin_price - strike_price),
            MarketRenderData::Resolved {
                strike_price: Some(strike_price),
                final_bitcoin_price,
                ..
            } => Some(final_bitcoin_price - strike_price),
            _ => None
        };

        let delta_string = format!(
            " Delta: {}",
            delta.map_or(Cow::Borrowed("---"), |d| Cow::Owned(format!("{:+.2}", d)))
        );

        let delta_color = if let Some(delta) = delta
        {
            if delta.is_sign_positive()
            {
                Color::Green
            }
            else
            {
                Color::Red
            }
        }
        else
        {
            Color::Gray
        };

        (delta_string, delta_color)
    };

    let (expiry_status_string, expiry_status_color) = match data
    {
        MarketRenderData::Active {
            time_untill_expiry, ..
        } =>
        {
            (
                Cow::Owned(format!(
                    " Market Expiring in {}.{}s",
                    time_untill_expiry.num_seconds(),
                    time_untill_expiry.num_milliseconds() % 1000
                )),
                if time_untill_expiry.as_seconds_f64() > 300.0
                {
                    Color::White
                }
                else if time_untill_expiry.as_seconds_f64() > 60.0
                {
                    Color::Yellow
                }
                else
                {
                    Color::Red
                }
            )
        }
        MarketRenderData::Resolving {
            time_after_expiry, ..
        } =>
        {
            (
                Cow::Owned(format!(
                    " Market Resolving {}.{}s elapsed",
                    time_after_expiry.num_seconds(),
                    time_after_expiry.num_milliseconds() % 1000
                )),
                Color::White
            )
        }
        MarketRenderData::Resolved {
            ..
        } => (Cow::Borrowed(" Market Resolved"), delta_color)
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![Span::styled(
                format!(
                    " Kalshi {} minute terminal | {}",
                    MARKET_INTERVAL_MINUTES,
                    data.get_market_id()
                ),
                Style::default().fg(ratatui::style::Color::Magenta)
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled(strike_price_string, Style::default().fg(Color::Blue)),
                Span::styled(bitcoin_price_string, Style::default().fg(Color::Yellow)),
                Span::styled(delta_string, Style::default().fg(delta_color)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                expiry_status_string,
                Style::default().fg(expiry_status_color)
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White))
        ),
        area
    );
}

fn render_orderbook(frame: &mut Frame, area: Rect, data: &MarketRenderData)
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

    // Asks are derived from No Bids (Negative Shares in our array).
    // We iterate forward (lowest price first) to find the best asks.
    let mut asks: Vec<(f64, i32)> = orderbook
        .data
        .iter()
        .enumerate()
        .filter_map(|(idx, &shares)| {
            if shares < 0
            {
                let cents = index_to_dollars(idx).unwrap_or(0.0) * 100.0;
                Some((cents, shares.abs()))
            }
            else
            {
                None
            }
        })
        .take(8)
        .collect();

    // Reverse so the highest ask price is at the top of the terminal
    asks.reverse();

    // Bids are Yes Bids (Positive Shares).
    // We iterate backwards (highest price first) to find the best bids.
    let bids: Vec<(f64, i32)> = orderbook
        .data
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(idx, &shares)| {
            if shares > 0
            {
                let cents = index_to_dollars(idx).unwrap_or(0.0) * 100.0;
                Some((cents, shares))
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
        (Some(ask), Some(bid)) =>
        {
            let spread = ask - bid;
            if spread > 0.0
            {
                format!("{:>4.1}¢", spread)
            }
            else
            {
                " - ".to_string()
            }
        }
        _ => " - ".to_string()
    };

    let empty = || {
        Line::from(Span::styled(
            "         - │ -                ",
            Style::default().fg(Color::DarkGray)
        ))
    };
    let separator = Line::from(Span::styled(
        "───────────┼──────────────────",
        Style::default().fg(Color::DarkGray)
    ));

    let mut lines = vec![
        Line::from(Span::styled(
            "   ASKS (Sell YES / Buy NO)   ",
            Style::default().fg(Color::Red)
        )),
        Line::from(Span::styled(
            " Price (¢) │ Size             ",
            Style::default().fg(Color::DarkGray)
        )),
        separator.clone(),
    ];

    // Pad asks if there are fewer than 8
    for _ in 0..8usize.saturating_sub(asks.len())
    {
        lines.push(empty());
    }

    // Render Asks
    for (cents, shares) in &asks
    {
        lines.push(Line::from(Span::styled(
            format!("{:>9.1} │ {:<16}", cents, shares),
            Style::default().fg(Color::LightRed)
        )));
    }

    // Render Spread
    lines.push(Line::from(Span::styled(
        format!("── SPREAD: {:<5} ─────────────", spread_s),
        Style::default().fg(Color::Yellow)
    )));

    // Render Bids
    for (cents, shares) in &bids
    {
        lines.push(Line::from(Span::styled(
            format!("{:>9.1} │ {:<16}", cents, shares),
            Style::default().fg(Color::LightGreen)
        )));
    }

    // Pad bids if there are fewer than 8
    for _ in 0..8usize.saturating_sub(bids.len())
    {
        lines.push(empty());
    }

    lines.push(separator);
    lines.push(Line::from(Span::styled(
        "   BIDS (Buy YES / Sell NO)   ",
        Style::default().fg(Color::Green)
    )));

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)) /* Adjust this to match your
                                                                  * border variable */
        ),
        area
    );
}

fn render_chart(frame: &mut Frame, area: Rect, data: &MarketRenderData)
{
    let mut min_delta: Option<f64> = None;
    let mut max_delta: Option<f64> = None;

    let data_to_render: Box<[(f64, f64); DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE]> = (0
        ..DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE)
        .map(|idx| {
            let delta = data.get_delta_history()[idx];

            if let Some(v) = &mut min_delta
            {
                *v = v.min(delta);
            }
            else
            {
                min_delta = Some(delta);
            }

            if let Some(v) = &mut max_delta
            {
                *v = v.max(delta);
            }
            else
            {
                max_delta = Some(delta);
            }

            (
                idx as f64 / DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE as f64,
                delta
            )
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();

    let min_delta = min_delta.unwrap();
    let max_delta = max_delta.unwrap();

    let dataset = ratatui::widgets::Dataset::default()
        .marker(ratatui::symbols::Marker::Braille)
        .style(Style::default().fg(Color::Cyan))
        .graph_type(ratatui::widgets::GraphType::Line)
        .data(&*data_to_render);

    frame.render_widget(
        ratatui::widgets::Chart::new(vec![dataset])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::White))
                    .title(" Delta History (15m) ")
            )
            .x_axis(ratatui::widgets::Axis::default().bounds([0.0, 1.0]))
            .y_axis(
                ratatui::widgets::Axis::default()
                    .bounds([min_delta, max_delta])
                    .labels(vec![
                        Span::raw(format!("{:.1}", min_delta)),
                        Span::raw(format!("{:.1}", max_delta)),
                    ])
            ),
        area
    );
}
