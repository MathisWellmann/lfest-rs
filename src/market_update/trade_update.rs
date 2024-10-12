use num_traits::Zero;

use super::MarketUpdate;
use crate::{
    order_filters::{enforce_max_price, enforce_min_price, enforce_step_size},
    prelude::{
        CurrencyMarker, LimitOrder, MarketState, Mon, Monies, Pending, PriceFilter, Quote, Side,
    },
    utils::min,
    Result,
};

/// A taker trade that consumes liquidity in the book.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Trade<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    /// The price at which the trade executed at.
    pub price: Monies<T, Quote>,
    /// The executed quantity.
    /// Generic denotation, e.g either Quote or Base currency denoted.
    pub quantity: Monies<T, BaseOrQuote>,
    /// Either a buy or sell order.
    // TODO: remove field and derive from sign of `quantity` to save size of struct.
    pub side: Side,
}

impl<T, BaseOrQuote, UserOrderId> MarketUpdate<T, BaseOrQuote, UserOrderId>
    for Trade<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
    UserOrderId: Clone,
{
    fn limit_order_filled(
        &self,
        order: &LimitOrder<T, BaseOrQuote, UserOrderId, Pending<T, BaseOrQuote>>,
    ) -> Option<Monies<T, BaseOrQuote>> {
        assert!(
            self.quantity > Monies::zero(),
            "The trade quantity must be greater than zero."
        );
        assert!(order.remaining_quantity() > Monies::zero());

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

    fn validate_market_update(&self, price_filter: &PriceFilter<T>) -> Result<(), T> {
        enforce_min_price(price_filter.min_price(), self.price)?;
        enforce_max_price(price_filter.max_price(), self.price)?;
        enforce_step_size(price_filter.tick_size(), self.price)?;
        Ok(())
    }

    fn update_market_state(&self, _market_state: &mut MarketState<T>) {}
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
    use fpdec::Decimal;

    use super::*;
    use crate::{
        base,
        prelude::{BaseCurrency, ExchangeOrderMeta},
        quote,
    };

    #[test_case::test_matrix(
        [100, 110, 120],
        [1, 2, 3],
        [Side::Buy, Side::Sell]
    )]
    fn trade_limit_order_filled_some(price: u32, qty: u32, side: Side) {
        let price = QuoteCurrency::new(Decimal::from(price));
        let quantity = BaseCurrency::new(Decimal::from(qty));
        let trade = Trade {
            price,
            quantity,
            side,
        };

        let offset = match side {
            Side::Buy => quote!(-1),
            Side::Sell => quote!(1),
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
    fn trade_limit_order_filled_none(price: u32, qty: u32, side: Side) {
        let price = QuoteCurrency::new(Decimal::from(price));
        let quantity = BaseCurrency::new(Decimal::from(qty));
        let trade = Trade {
            price,
            quantity,
            side,
        };
        let offset = match side {
            Side::Buy => quote!(-1),
            Side::Sell => quote!(1),
        };
        let limit_order =
            LimitOrder::new(side.inverted(), price + offset, quantity / base!(2)).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let limit_order = limit_order.into_pending(meta);
        assert_eq!(
            trade.limit_order_filled(&limit_order).unwrap(),
            quantity / base!(2)
        );
    }

    #[test]
    fn size_of_trade() {
        assert_eq!(std::mem::size_of::<Trade<BaseCurrency>>(), 80);
    }
}
