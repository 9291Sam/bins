use anyhow::Context;

/// Tapered-deci-cent#
#[derive(Clone)]
pub struct Orderbook
{
    // 0..=100   (101 slots) -> [$0.000, $0.100] (0.001 step)
    // 101..=179 (79 slots)  -> [$0.110, $0.890] (0.010 step)
    // 180..=280 (101 slots) -> [$0.900, $1.000] (0.001 step)
    pub data: [i32; 281]
}

impl Orderbook
{
    pub fn new() -> Orderbook
    {
        Orderbook {
            data: [0; 281]
        }
    }

    pub fn get_shares(&self, dollars: f64) -> i32
    {
        self.data[get_index_of_dollars(dollars)
            .with_context(|| format!("Tried to get index of ${dollars}"))
            .unwrap()]
    }

    pub fn set_shares(&mut self, dollars: f64, shares: i32)
    {
        self.data[get_index_of_dollars(dollars)
            .with_context(|| format!("Tried to get index of ${dollars}"))
            .unwrap()] = shares;
    }

    pub fn add_shares(&mut self, dollars: f64, shares: i32)
    {
        self.data[get_index_of_dollars(dollars)
            .with_context(|| format!("Tried to get index of ${dollars}"))
            .unwrap()] += shares;
    }
}

pub fn get_index_of_dollars(dollars: f64) -> Option<usize>
{
    if !(0.0..=1.0).contains(&dollars)
    {
        return None;
    }

    let mils = (dollars * 1000.0).round() as i32;

    match mils
    {
        // Range 1: $0.000 to $0.100 (Step: $0.001)
        0..=100 => Some(mils as usize),

        // Range 2: $0.110 to $0.890 (Step: $0.010)
        110..=890 =>
        {
            // Must fall exactly on a $0.01 step (meaning mils must be a multiple of 10)
            if mils % 10 == 0
            {
                Some(101 + ((mils - 110) / 10) as usize)
            }
            else
            {
                None // Invalid step size, e.g., $0.115
            }
        }

        // Range 3: $0.900 to $1.000 (Step: $0.001)
        900..=1000 => Some(180 + (mils - 900) as usize),

        _ => None
    }
}

pub fn index_to_dollars(idx: usize) -> Option<f64>
{
    match idx
    {
        0..=100 => Some(idx as f64 / 1000.0),
        101..=179 => Some(0.110 + ((idx - 101) as f64 / 100.0)),
        180..=280 => Some(0.900 + ((idx - 180) as f64 / 1000.0)),
        _ => None
    }
}
