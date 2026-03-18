use std::ops::RangeInclusive;

/// Tapered-deci-cent
struct Orderbook
{
    // 0..=100   (101 slots) -> [0.0, 10.0] cents (0.1 step)
    // 101..=179 (79 slots)  -> [11.0, 89.0] cents (1.0 step)
    // 180..=280 (101 slots) -> [90.0, 100.0] cents (0.1 step)
    data: [i32; 281]
}

impl Orderbook
{
    pub fn new() -> Orderbook {}

    pub fn get_shares(cents: f64) {}
}

fn get_index_of_cents(cents: f64) -> Option<usize>
{
    if !(0.0..=1.0).contains(&cents)
    {
        return None;
    }

    if (cents % 0.001).abs() > 1e-12
    {
        return None;
    }

    let tenths = (cents * 10.0) as usize;

    match tenths
    {
        0..=100 => Some(tenths),
        101..=179 =>
        {
            if tenths % 10 == 0
            {
                Some((tenths - 110) / 10)
            }
            else
            {
                None
            }
        }
        180..=280 => Some(180 + (tenths - 900) as usize),
        _ => None
    }
}

pub fn index_to_cents(idx: usize) -> Option<f64>
{
    match idx
    {
        0..=100 => Some(idx as f64 / 10.0),
        101..=179 => Some(11.0 + (idx - 101) as f64),
        180..=280 => Some(90.0 + (idx - 180) as f64 / 10.0),
        _ => None
    }
}

fn warn_on_invalid_float(cents: f64) {}
