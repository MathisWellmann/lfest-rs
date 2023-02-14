use std::fmt::Formatter;

/// Fee as a fraction
/// TODO: make generic
#[derive(Default, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Fee(pub f64);

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

/// Decribes the possible updates to the market state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MarketUpdate {
    /// An update to the best bid and ask has occured
    Bba {
        /// The new best bid
        bid: QuoteCurrency,
        /// The new best ask
        ask: QuoteCurrency,
    },
    /// A new candle has been created
    Candle {
        /// The best bid at the time of candle creation
        bid: QuoteCurrency,
        /// The best ask at the time of candle creation
        ask: QuoteCurrency,
        /// The low price of the candle
        low: QuoteCurrency,
        /// The high price of the candle
        high: QuoteCurrency,
    },
}

/// Creates the MarketUpdate::Bba variant
#[macro_export]
macro_rules! bba {
    ( $b:expr, $a:expr ) => {{
        MarketUpdate::Bba {
            bid: $b,
            ask: $a,
        }
    }};
}

/// Creates the MarketUpdate::Candle variant
#[macro_export]
macro_rules! candle {
    ( $b:expr, $a:expr, $l:expr, $h:expr ) => {{
        MarketUpdate::Candle {
            bid: $b,
            ask: $a,
            low: $l,
            high: $h,
        }
    }};
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
/// Side of the order
pub enum Side {
    /// Buy side
    Buy,
    /// Sell side
    Sell,
}

impl Side {
    /// Returns the inverted side
    pub fn inverted(&self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
/// Defines the available order types
pub enum OrderType {
    /// aggressive market order
    Market,
    /// passive limit order
    Limit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bba_macro() {
        let m = bba!(100.0, 100.1);

        assert_eq!(
            m,
            MarketUpdate::Bba {
                bid: 100.0,
                ask: 100.1
            }
        );
    }

    #[test]
    fn candle_macro() {
        let c = candle!(100.0, 100.1, 100.0, 100.1);

        assert_eq!(
            c,
            MarketUpdate::Candle {
                bid: 100.0,
                ask: 100.1,
                low: 100.0,
                high: 100.1
            }
        )
    }
}
