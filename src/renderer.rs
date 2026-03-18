use std::borrow::Cow;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::{DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE, MARKET_INTERVAL_MINUTES};

pub enum MarketRenderData
{
    Active
    {
        strike_price:          Option<f64>,
        current_bitcoin_price: f64,
        market_id:             String,
        time_untill_expiry:    chrono::Duration,
        orderbook_shares:      crate::OrderBookShares,
        delta_history:         crate::DeltaHistory
    },
    Resolving
    {
        strike_price:                 Option<f64>,
        estimate_final_bitcoin_price: f64,
        market_id:                    String,
        time_after_expiry:            chrono::Duration,
        orderbook_shares:             crate::OrderBookShares,
        delta_history:                crate::DeltaHistory
    },
    Resolved
    {
        strike_price:        Option<f64>,
        final_bitcoin_price: f64,
        market_id:           String,
        delta_history:       crate::DeltaHistory
    }
}

impl MarketRenderData
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

    pub fn get_delta_history(&self) -> &crate::DeltaHistory
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
        } => format!(" Live Bitcoin Price: ${current_bitcoin_price}"),
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
    const ORDERBOOK_ELEMENTS_ON_EACH_SIDE_OF_SPREAD: usize = 8;

    struct SharesAtPrice
    {
        price:  u8,
        shares: i32
    }

    let (orderbook_shares_and_prices, spread_cents): (
        [Option<SharesAtPrice>; ORDERBOOK_ELEMENTS_ON_EACH_SIDE_OF_SPREAD * 2],
        u8
    ) = {
        let orderbook: &crate::OrderBookShares = match data
        {
            MarketRenderData::Active {
                orderbook_shares, ..
            } => orderbook_shares,
            MarketRenderData::Resolving {
                orderbook_shares, ..
            } => orderbook_shares,
            MarketRenderData::Resolved {
                ..
            } => return
        };

        let positive_min_index = orderbook
            .iter()
            .enumerate()
            .filter(|(_, x)| **x > 0)
            .map(|(idx, _)| idx)
            .next()
            .unwrap();

        let positive_max_index = (positive_min_index + ORDERBOOK_ELEMENTS_ON_EACH_SIDE_OF_SPREAD
            - 1)
        .min(orderbook.len() - 1);

        let negative_max_index = orderbook
            .iter()
            .enumerate()
            .filter(|(_, x)| **x < 0)
            .map(|(idx, _)| idx)
            .next_back()
            .unwrap();

        let negative_min_index =
            negative_max_index.saturating_sub(ORDERBOOK_ELEMENTS_ON_EACH_SIDE_OF_SPREAD - 1);

        let positive_indices = positive_min_index..=positive_max_index;
        let negative_indices = negative_min_index..=negative_max_index;

        let spread_cents = positive_min_index.saturating_sub(negative_max_index);

        let mut orderbook_shares_and_prices: [Option<SharesAtPrice>;
            ORDERBOOK_ELEMENTS_ON_EACH_SIDE_OF_SPREAD * 2] = [const { None }; _];

        for (i, positive_index) in positive_indices.enumerate()
        {
            orderbook_shares_and_prices[i + ORDERBOOK_ELEMENTS_ON_EACH_SIDE_OF_SPREAD] =
                Some(SharesAtPrice {
                    price:  positive_index as u8,
                    shares: orderbook[positive_index]
                });
        }

        for (i, negative_index) in negative_indices.enumerate()
        {
            orderbook_shares_and_prices[i] = Some(SharesAtPrice {
                price:  negative_index as u8,
                shares: orderbook[negative_index]
            });
        }

        (orderbook_shares_and_prices, spread_cents as u8)
    };

    let mut shares_strings: Vec<Cow<'static, str>> = Vec::new();
    shares_strings.reserve_exact(ORDERBOOK_ELEMENTS_ON_EACH_SIDE_OF_SPREAD * 2);

    for e in orderbook_shares_and_prices.iter().rev()
    {
        match e
        {
            Some(SharesAtPrice {
                price,
                shares
            }) => shares_strings.push(Cow::Owned(format!("{price}¢ │ {shares}",))),
            None => shares_strings.push(Cow::Borrowed("--- │ ---"))
        };
    }

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "Asks (Sell Yes / Buy No)",
            Style::default().fg(Color::Red)
        )),
        Line::from(Span::styled(
            "Price (¢) │ Shares",
            Style::default().fg(Color::Red)
        )),
    ];

    for s in shares_strings
        .iter()
        .rev()
        .take(ORDERBOOK_ELEMENTS_ON_EACH_SIDE_OF_SPREAD)
    {
        lines.push(Line::from(Span::styled(
            s.clone(),
            Style::default().fg(Color::LightRed)
        )));
    }

    lines.push(Line::from(Span::styled(
        format!("─── Spread: {spread_cents}¢ ───"),
        Style::default().fg(Color::Yellow)
    )));

    for s in shares_strings
        .iter()
        .take(ORDERBOOK_ELEMENTS_ON_EACH_SIDE_OF_SPREAD)
        .rev()
    {
        lines.push(Line::from(Span::styled(
            s.clone(),
            Style::default().fg(Color::LightGreen)
        )));
    }

    lines.push(Line::from(Span::styled(
        "Asks (Sell No / Buy Yes)",
        Style::default().fg(Color::Red)
    )));

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White))
        ),
        area
    );
}

fn render_chart(frame: &mut Frame, area: Rect, data: &MarketRenderData)
{
    let mut min_delta: Option<f64> = None;
    let mut max_delta: Option<f64> = None;

    let data_to_render: [(f64, f64); DISCRETE_TIMESTEPS_TO_SAVE_PER_EPISODE] =
        std::array::from_fn(|idx| {
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
        });

    let min_delta = min_delta.unwrap();
    let max_delta = max_delta.unwrap();

    let dataset = ratatui::widgets::Dataset::default()
        .marker(ratatui::symbols::Marker::Braille)
        .style(Style::default().fg(Color::Cyan))
        .graph_type(ratatui::widgets::GraphType::Line)
        .data(&data_to_render);

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
