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
            self.quantity != Q::new_zero(),
            "The trade quantity must not be zero"
        );

        if match order.side() {
            Side::Buy => self.price <= order.limit_price() && matches!(self.side, Side::Sell),
            Side::Sell => self.price >= order.limit_price() && matches!(self.side, Side::Buy),
        } {
            // Execute up to the quantity of the incoming `Trade`.
            let filled_qty = min(self.quantity, order.remaining_quantity());
            Some(filled_qty)
        } else {
            None
        }
    }

    fn validate_market_update(&self, price_filter: &PriceFilter) -> Result<()> {
        enforce_min_price(price_filter.min_price, self.price)?;
        enforce_max_price(price_filter.max_price, self.price)?;
        enforce_step_size(price_filter.tick_size, self.price)?;
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
