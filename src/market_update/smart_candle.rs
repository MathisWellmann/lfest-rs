use super::{MarketUpdate, Trade};
use crate::types::{Currency, Mon, QuoteCurrency, Side, UserOrderIdT};

/// A datastructure for aggregated trades with the ability to approximate realistic taker fill flow.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SmartCandle<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    aggregate_buy_volume: Vec<(QuoteCurrency<I, D>, BaseOrQuote)>,
    aggregate_sell_volume: Vec<(QuoteCurrency<I, D>, BaseOrQuote)>,
}

impl<I, const D: u8, BaseOrQuote> SmartCandle<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// Create a new instance, converting taker trades into an efficient structure.
    pub fn new(taker_trades: &[Trade<I, D, BaseOrQuote>]) -> Self {
        assert2::assert!(!taker_trades.is_empty());

        // split buy and sell flow.
        let mut buys = Vec::with_capacity(taker_trades.len());
        let mut sells = Vec::with_capacity(taker_trades.len());
        // TODO: Relax this assertion.
        assert2::assert!(buys.is_empty());
        assert2::assert!(sells.is_empty());

        for trade in taker_trades {
            // only retain the most important stuff.
            assert2::debug_assert!(trade.quantity > BaseOrQuote::zero());
            match trade.side {
                Side::Buy => buys.push((trade.price, trade.quantity)),
                Side::Sell => sells.push((trade.price, trade.quantity)),
            }
        }

        // sort by prices.
        glidesort::sort_by_key(&mut buys, |t| -t.0); // Reverse is easier here.
        glidesort::sort_by_key(&mut sells, |t| t.0);

        // aggregate price levels, summing up the quantities.
        let mut aggregate_buy_volume = Vec::with_capacity(10);
        let mut last_buy_price = buys[0].0;
        let mut buy_volume_sum = BaseOrQuote::zero();
        // Largest prices first.
        for (buy_price, buy_qty) in buys {
            if buy_price != last_buy_price {
                aggregate_buy_volume.push((last_buy_price, buy_volume_sum));
                last_buy_price = buy_price
            }
            buy_volume_sum += buy_qty;
        }
        aggregate_buy_volume.push((last_buy_price, buy_volume_sum));

        let mut aggregate_sell_volume = Vec::with_capacity(10);
        let mut last_sell_price = sells[0].0;
        let mut sell_volume_sum = BaseOrQuote::zero();
        // Smallest prices first
        for (sell_price, sell_qty) in sells {
            if sell_price != last_sell_price {
                aggregate_sell_volume.push((last_sell_price, sell_volume_sum));
                last_sell_price = sell_price;
            }
            sell_volume_sum += sell_qty;
        }
        aggregate_sell_volume.push((last_sell_price, sell_volume_sum));

        Self {
            aggregate_buy_volume,
            aggregate_sell_volume,
        }
    }
}

impl<I, const D: u8, BaseOrQuote> std::fmt::Display for SmartCandle<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderId> MarketUpdate<I, D, BaseOrQuote, UserOrderId>
    for SmartCandle<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderId: UserOrderIdT,
{
    const CAN_FILL_LIMIT_ORDERS: bool = true;

    fn limit_order_filled(
        &self,
        limit_order: &crate::prelude::LimitOrder<
            I,
            D,
            BaseOrQuote,
            UserOrderId,
            crate::prelude::Pending<I, D, BaseOrQuote>,
        >,
    ) -> Option<BaseOrQuote> {
        match limit_order.side() {
            Side::Buy => todo!(),
            Side::Sell => todo!(),
        }
    }

    fn validate_market_update(
        &self,
        price_filter: &crate::prelude::PriceFilter<I, D>,
    ) -> crate::Result<()> {
        todo!()
    }

    fn update_market_state(&self, market_state: &mut crate::prelude::MarketState<I, D>) {
        todo!()
    }

    fn timestamp_exchange_ns(&self) -> crate::prelude::TimestampNs {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BaseCurrency;

    #[test]
    fn smart_candle() {
        let trades = &[
            Trade {
                timestamp_exchange_ns: 0.into(),
                price: QuoteCurrency::<i64, 5>::new(100, 0),
                quantity: BaseCurrency::new(2, 0),
                side: Side::Buy,
            },
            Trade {
                timestamp_exchange_ns: 0.into(),
                price: QuoteCurrency::<i64, 5>::new(100, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Sell,
            },
        ];
        let smart_candle = SmartCandle::new(trades);

        assert_eq!(
            smart_candle,
            SmartCandle {
                aggregate_buy_volume: vec![(QuoteCurrency::new(100, 0), BaseCurrency::new(2, 0))],
                aggregate_sell_volume: vec![(QuoteCurrency::new(100, 0), BaseCurrency::new(1, 0))]
            }
        )
    }
}
