/// The markets BASE currency, e.g.: BTCUSD -> BTC is the BASE currency
/// TODO: make inner type private and create getter and setter
/// TODO: make malachite type / generic
#[derive(Default, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BaseCurrency(pub f64);

/// The markets QUOTE currency, e.g.: BTCUSD -> USD is the quote currency
/// TODO: make inner type private and create getter and setter
/// TODO: make malachite type / generic
#[derive(Default, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QuoteCurrency(pub f64);

pub trait Currency: Copy + std::fmt::Debug {
    fn val(&self) -> f64;
}

impl Currency for BaseCurrency {
    fn val(&self) -> f64 {
        self.0
    }
}

impl Currency for QuoteCurrency {
    fn val(&self) -> f64 {
        self.0
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
