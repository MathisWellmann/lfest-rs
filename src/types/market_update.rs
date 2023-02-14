use crate::QuoteCurrency;

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
