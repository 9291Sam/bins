use anyhow::Context;

#[derive(Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Orderbook
{
    pub data: [i32; 281]
}

impl Orderbook
{
    #[allow(clippy::new_without_default)]
    pub fn new() -> Orderbook
    {
        Orderbook {
            data: [0; 281]
        }
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

    pub fn get_best_ask_dollars(&self) -> Option<f64>
    {
        self.data.iter().enumerate().find_map(|(idx, &shares)| {
            if shares < 0
            {
                index_to_dollars(idx)
            }
            else
            {
                None
            }
        })
    }

    pub fn get_best_bid_dollars(&self) -> Option<f64>
    {
        self.data
            .iter()
            .enumerate()
            .rev()
            .find_map(|(idx, &shares)| {
                if shares > 0
                {
                    index_to_dollars(idx)
                }
                else
                {
                    None
                }
            })
    }

    pub fn get_mid_cents(&self) -> Option<f64>
    {
        let ask = self.get_best_ask_dollars()?;
        let bid = self.get_best_bid_dollars()?;
        Some((ask + bid) / 2.0 * 100.0)
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
        0..=100 => Some(mils as usize),
        110..=890 =>
        {
            if mils % 10 == 0
            {
                Some(101 + ((mils - 110) / 10) as usize)
            }
            else
            {
                None
            }
        }
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
