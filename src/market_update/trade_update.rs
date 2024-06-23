use super::MarketUpdate;
use crate::{
    order_filters::{enforce_max_price, enforce_min_price, enforce_step_size},
    prelude::{Currency, LimitOrder, MarketState, Pending, PriceFilter, QuoteCurrency, Side},
    utils::min,
    Result,
};

/// A taker trade that consumes liquidity in the book.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Trade<Q> {
    /// The price at which the trade executed at.
    pub price: QuoteCurrency,
    /// The executed quantity.
    /// Generic denotation, e.g either Quote or Base currency denoted.
    pub quantity: Q,
    /// Either a buy or sell order.
    pub side: Side,
}

impl<Q, UserOrderId> MarketUpdate<Q, UserOrderId> for Trade<Q>
where
    Q: Currency,
    UserOrderId: Clone,
{
    fn limit_order_filled(&self, order: &LimitOrder<Q, UserOrderId, Pending<Q>>) -> Option<Q> {
        assert!(
            self.quantity > Q::new_zero(),
            "The trade quantity must be greater than zero."
        );
        assert!(order.remaining_quantity() > Q::new_zero());

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

    fn validate_market_update(&self, price_filter: &PriceFilter) -> Result<()> {
        enforce_min_price(price_filter.min_price(), self.price)?;
        enforce_max_price(price_filter.max_price(), self.price)?;
        enforce_step_size(price_filter.tick_size(), self.price)?;
        Ok(())
    }

    fn update_market_state(&self, _market_state: &mut MarketState) {}
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
}
