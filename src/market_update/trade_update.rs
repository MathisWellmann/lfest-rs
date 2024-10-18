use super::MarketUpdate;
use crate::{
    order_filters::{enforce_max_price, enforce_min_price, enforce_step_size},
    prelude::{
        CurrencyMarker, LimitOrder, MarketState, Mon, Pending, PriceFilter, QuoteCurrency, Side,
    },
    utils::min,
    Result,
};

/// A taker trade that consumes liquidity in the book.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Trade<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker<I, D>,
{
    /// The price at which the trade executed at.
    pub price: QuoteCurrency<I, D>,
    /// The executed quantity.
    /// Generic denotation, e.g either Quote or Base currency denoted.
    pub quantity: BaseOrQuote,
    /// Either a buy or sell order.
    // TODO: remove field and derive from sign of `quantity` to save size of struct.
    pub side: Side,
}

impl<I, const D: u8, BaseOrQuote> std::fmt::Display for Trade<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker<I, D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "price {}, quantity: {}, side: {}",
            self.price, self.quantity, self.side
        )
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderId> MarketUpdate<I, D, BaseOrQuote, UserOrderId>
    for Trade<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: CurrencyMarker<I, D>,
    UserOrderId: Clone,
{
    fn limit_order_filled(
        &self,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>,
    ) -> Option<BaseOrQuote> {
        assert!(
            self.quantity > BaseOrQuote::zero(),
            "The trade quantity must be greater than zero."
        );
        assert!(order.remaining_quantity() > BaseOrQuote::zero());

        // Notice that the limit order price must be strictly lower or higher than the limit order price,
        // because we assume the limit order has the worst possible queue position in the book.
        if match order.side() {
            Side::Buy => self.price < order.limit_price() && matches!(self.side, Side::Sell),
            Side::Sell => self.price > order.limit_price() && matches!(self.side, Side::Buy),
        } {
            // Execute up to the quantity of the incoming `Trade`.
            let filled_qty = min(self.quantity, order.remaining_quantity());
            Some(filled_qty)
        } else {
            None
        }
    }

    fn validate_market_update(&self, price_filter: &PriceFilter<I, D>) -> Result<(), I, D> {
        enforce_min_price(price_filter.min_price(), self.price)?;
        enforce_max_price(price_filter.max_price(), self.price)?;
        enforce_step_size(price_filter.tick_size(), self.price)?;
        Ok(())
    }

    fn update_market_state(&self, _market_state: &mut MarketState<I, D>) {}
}
/// Creates the `Trade` struct used as a `MarketUpdate`.
#[macro_export]
macro_rules! trade {
    ( $price:expr, $quantity:expr, $side:expr ) => {{
        $crate::prelude::Trade {
            price: $price,
            quantity: $quantity,
            side: $side,
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test_case::test_matrix(
        [100, 110, 120],
        [1, 2, 3],
        [Side::Buy, Side::Sell]
    )]
    fn trade_limit_order_filled_some(price: i32, qty: i32, side: Side) {
        let price = QuoteCurrency::<i32, 2>::new(price, 0);
        let quantity = BaseCurrency::new(qty, 0);
        let trade = Trade {
            price,
            quantity,
            side,
        };

        let offset = match side {
            Side::Buy => QuoteCurrency::new(-1, 0),
            Side::Sell => QuoteCurrency::new(1, 0),
        };
        let limit_order = LimitOrder::new(side.inverted(), price + offset, quantity).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let limit_order = limit_order.into_pending(meta);
        assert_eq!(trade.limit_order_filled(&limit_order).unwrap(), quantity);
    }

    #[test_case::test_matrix(
        [100, 110, 120],
        [1, 2, 3],
        [Side::Buy, Side::Sell]
    )]
    fn trade_limit_order_filled_none(price: i32, qty: i32, side: Side) {
        let price = QuoteCurrency::<i32, 2>::new(price, 0);
        let quantity = BaseCurrency::new(qty, 0);
        let trade = Trade {
            price,
            quantity,
            side,
        };
        let offset = match side {
            Side::Buy => QuoteCurrency::new(-1, 0),
            Side::Sell => QuoteCurrency::new(1, 0),
        };
        let limit_order = LimitOrder::new(
            side.inverted(),
            price + offset,
            quantity / BaseCurrency::new(2, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let limit_order = limit_order.into_pending(meta);
        assert_eq!(
            trade.limit_order_filled(&limit_order).unwrap(),
            quantity / BaseCurrency::new(2, 0)
        );
    }

    #[test]
    fn size_of_trade() {
        assert_eq!(
            std::mem::size_of::<Trade<i32, 2, BaseCurrency<i32, 2>>>(),
            12
        );
        assert_eq!(
            std::mem::size_of::<Trade<i64, 2, BaseCurrency<i64, 2>>>(),
            24
        );
    }
}
