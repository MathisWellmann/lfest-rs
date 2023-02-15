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

pub trait Currency:
    Copy + std::fmt::Debug + std::fmt::Display + std::ops::Add + std::ops::Sub + PartialEq + PartialOrd
{
    /// The paired currency.
    /// e.g.: for the BTCUSD market the BTC currency is paired with USD, so the `PairedCurrency` would be USD
    type PairedCurrency: Currency;

    /// Check if the value is zero
    fn is_zero(&self) -> bool;

    /// Create a new currency instance with zero value
    fn new_zero() -> Self;

    #[cfg(test)]
    fn into_rounded(self, prec: i32) -> Self;
}

impl Currency for BaseCurrency {
    type PairedCurrency = QuoteCurrency;

    fn is_zero(&self) -> bool {
        self.0 == 0.0
    }

    fn new_zero() -> Self {
        Self(0.0)
    }

    #[cfg(test)]
    fn into_rounded(self, prec: i32) -> Self {
        Self(crate::utils::round(self.0, prec))
    }
}

impl Currency for QuoteCurrency {
    type PairedCurrency = BaseCurrency;

    fn is_zero(&self) -> bool {
        self.0 == 0.0
    }

    fn new_zero() -> Self {
        Self(0.0)
    }

    #[cfg(test)]
    fn into_rounded(self, prec: i32) -> Self {
        Self(crate::utils::round(self.0, prec))
    }
}

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
