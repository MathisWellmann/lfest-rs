use derive_more::{Add, Display, Sub};

/// The markets BASE currency, e.g.: BTCUSD -> BTC is the BASE currency
/// TODO: make inner type private and create getter and setter
/// TODO: make malachite type / generic
#[derive(
    Default, Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Add, Sub, Display,
)]
pub struct BaseCurrency(pub f64);

/// The markets QUOTE currency, e.g.: BTCUSD -> USD is the quote currency
/// TODO: make inner type private and create getter and setter
/// TODO: make malachite type / generic
#[derive(
    Default, Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Add, Sub, Display,
)]
pub struct QuoteCurrency(pub f64);

pub trait Currency: Copy + std::fmt::Display + std::ops::Add + std::ops::Sub {}

impl Currency for BaseCurrency {}

impl Currency for QuoteCurrency {}

/// Allows the quick construction on `QuoteCurrency`
#[macro_export]
macro_rules! quote {
    ( $a:expr ) => {{
        QuoteCurrency($a)
    }};
}

/// Allows the quick construction on `BaseCurrency`
#[macro_export]
macro_rules! base {
    ( $a:expr ) => {{
        BaseCurrency($a)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_display() {
        println!("{}", base!(0.5));
    }
}
