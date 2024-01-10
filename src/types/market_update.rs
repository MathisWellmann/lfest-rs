use super::{Currency, Side};
use crate::types::QuoteCurrency;

/// Decribes the possible updates to the market state
#[derive(Debug, Clone, PartialEq)]
pub enum MarketUpdate<S>
where
    S: Currency,
{
    /// An update to the best bid and ask has occured.
    /// For now we don't handle the quantity a these price levels.
    /// This will change in future versions.
    Bba {
        /// The new best bid
        bid: QuoteCurrency,
        /// The new best ask
        ask: QuoteCurrency,
    },
    /// A taker trade that consumes liquidity in the book.
    ///
    Trade {
        /// The price at which the trade executed at.
        price: QuoteCurrency,
        /// The executed quantity.
        /// Generic denotation, e.g either Quote or Base currency denoted.
        quantity: S,
        /// Either a buy or sell order.
        side: Side,
    },
    /// A new candle has been created.
    /// Here we can use the `high` and `low` prices to see if our simulated resting orders
    /// have been executed over the last period as a proxy in absence of actual `Trade` flow.
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

/// Creates the `MarketUpdate::Bba` variant.
#[macro_export]
macro_rules! bba {
    ( $b:expr, $a:expr ) => {{
        $crate::prelude::MarketUpdate::Bba { bid: $b, ask: $a }
    }};
}

/// Creates the `MarketUpdate::Trade` variant.
#[macro_export]
macro_rules! trade {
    ( $price:expr, $quantity:expr, $side:expr ) => {{
        $crate::prelude::MarketUpdate::Trade {
            price: $price,
            quantity: $quantity,
            side: $side,
        }
    }};
}

/// Creates the `MarketUpdate::Candle! variant.
#[macro_export]
macro_rules! candle {
    ( $b:expr, $a:expr, $l:expr, $h:expr ) => {{
        $crate::prelude::MarketUpdate::Candle {
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
    use crate::prelude::*;

    #[test]
    fn bba_macro() {
        let m: MarketUpdate<BaseCurrency> = bba!(quote!(100.0), quote!(100.1));

        assert_eq!(
            m,
            MarketUpdate::Bba {
                bid: quote!(100.0),
                ask: quote!(100.1)
            }
        );
    }

    #[test]
    fn candle_macro() {
        let c: MarketUpdate<BaseCurrency> =
            candle!(quote!(100.0), quote!(100.1), quote!(100.0), quote!(100.1));

        assert_eq!(
            c,
            MarketUpdate::Candle {
                bid: quote!(100.0),
                ask: quote!(100.1),
                low: quote!(100.0),
                high: quote!(100.1),
            }
        )
    }
}
