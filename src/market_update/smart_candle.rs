use getset::{
    CopyGetters,
    Getters,
};

use super::{
    Bba,
    MarketUpdate,
    Trade,
};
use crate::{
    market_update::market_update_trait::Exhausted,
    prelude::PriceFilter,
    types::{
        Currency,
        Mon,
        PriceFilterError,
        QuoteCurrency,
        Side,
        TimestampNs,
        UserOrderId,
    },
    utils::min,
};

/// A data structure for aggregated trades with the ability to approximate realistic taker fill flow.
/// Basically a `Candle` buy one that does not blindly fill active limit orders with taker flow that does not exist.
#[derive(Debug, Clone, Eq, PartialEq, Getters, CopyGetters)]
pub struct SmartCandle<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// The highest price seen during the candle.
    #[getset(get_copy = "pub")]
    high: QuoteCurrency<I, D>,

    /// The lowest price seen during the candle.
    #[getset(get_copy = "pub")]
    low: QuoteCurrency<I, D>,

    /// Each price level contains the cumulative buy quantities of all higher price levels and the current one.
    aggregate_buy_volume: Vec<(QuoteCurrency<I, D>, BaseOrQuote)>,

    // Each price level contains the cumulative sell quanties of all lower price levels and the current one.
    aggregate_sell_volume: Vec<(QuoteCurrency<I, D>, BaseOrQuote)>,

    /// The best bid and ask information
    #[getset(get_copy = "pub")]
    bba: Bba<I, D>,

    /// The last timestamp in nanoseconds
    #[getset(get_copy = "pub")]
    last_timestamp_exchange_ns: TimestampNs,
}

impl<I, const D: u8, BaseOrQuote> SmartCandle<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// Create a new instance, converting taker trades into an efficient structure.
    pub fn new(
        taker_trades: &[Trade<I, D, BaseOrQuote>],
        bba: Bba<I, D>,
        price_filter: &PriceFilter<I, D>,
    ) -> Self {
        assert!(!taker_trades.is_empty());

        debug_assert!(
            taker_trades
                .iter()
                .any(|t| t.validate_market_update(price_filter).is_ok())
        );
        <Bba<I, D> as MarketUpdate<I, D, BaseOrQuote>>::validate_market_update(&bba, price_filter)
            .expect("The `Bba` is correct");

        // split buy and sell flow.
        let mut buys = Vec::with_capacity(taker_trades.len());
        let mut sells = Vec::with_capacity(taker_trades.len());
        let mut high = taker_trades[0].price;
        let mut low = taker_trades[0].price;

        for trade in taker_trades {
            // only retain the most important stuff.
            assert2::debug_assert!(trade.quantity > BaseOrQuote::zero());
            #[allow(
                clippy::disallowed_methods,
                reason = "Don't know if we have enough capacity"
            )]
            match trade.side {
                Side::Buy => buys.push((trade.price, trade.quantity)),
                Side::Sell => sells.push((trade.price, trade.quantity)),
            }
            if trade.price < low {
                low = trade.price;
            }
            if trade.price > high {
                high = trade.price;
            }
        }

        // sort by prices.
        glidesort::sort_by_key(&mut buys, |t| -t.0); // Reverse is easier here.
        glidesort::sort_by_key(&mut sells, |t| t.0);

        // aggregate price levels, summing up the quantities.
        let mut aggregate_buy_volume = Vec::with_capacity(10);
        if !buys.is_empty() {
            let mut last_buy_price = buys[0].0;
            let mut buy_volume_sum = BaseOrQuote::zero();
            // Largest prices first.
            for (buy_price, buy_qty) in buys {
                if buy_price != last_buy_price {
                    #[allow(
                        clippy::disallowed_methods,
                        reason = "Don't know if we have enough capacity"
                    )]
                    aggregate_buy_volume.push((last_buy_price, buy_volume_sum));
                    last_buy_price = buy_price
                }
                buy_volume_sum += buy_qty;
            }
            #[allow(
                clippy::disallowed_methods,
                reason = "Don't know if we have enough capacity"
            )]
            aggregate_buy_volume.push((last_buy_price, buy_volume_sum));
        }

        let mut aggregate_sell_volume = Vec::with_capacity(10);
        if !sells.is_empty() {
            let mut last_sell_price = sells[0].0;
            let mut sell_volume_sum = BaseOrQuote::zero();
            // Smallest prices first
            for (sell_price, sell_qty) in sells {
                if sell_price != last_sell_price {
                    #[allow(
                        clippy::disallowed_methods,
                        reason = "Don't know if we have enough capacity"
                    )]
                    aggregate_sell_volume.push((last_sell_price, sell_volume_sum));
                    last_sell_price = sell_price;
                }
                sell_volume_sum += sell_qty;
            }
            #[allow(
                clippy::disallowed_methods,
                reason = "Don't know if we have enough capacity"
            )]
            aggregate_sell_volume.push((last_sell_price, sell_volume_sum));
        }

        Self {
            high,
            low,
            aggregate_buy_volume,
            aggregate_sell_volume,
            last_timestamp_exchange_ns: taker_trades[taker_trades.len() - 1].timestamp_exchange_ns,
            bba,
        }
    }
}

impl<I, const D: u8, BaseOrQuote> std::fmt::Display for SmartCandle<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SmartCandle with {} buy volume levels and {} sell volume levels. last_timestamp_exchange_ns: {}",
            self.aggregate_buy_volume.len(),
            self.aggregate_sell_volume.len(),
            self.last_timestamp_exchange_ns
        )
    }
}

impl<I, const D: u8, BaseOrQuote> MarketUpdate<I, D, BaseOrQuote> for SmartCandle<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    const CAN_FILL_LIMIT_ORDERS: bool = true;

    // TODO: benchmark and optimize this.
    // TODO: reduce the quantity if a limit order is filled.
    #[inline]
    fn limit_order_filled<UserOrderIdT: UserOrderId>(
        &mut self,
        limit_order: &crate::prelude::LimitOrder<
            I,
            D,
            BaseOrQuote,
            UserOrderIdT,
            crate::prelude::Pending<I, D, BaseOrQuote>,
        >,
    ) -> Option<(BaseOrQuote, Exhausted)> {
        match limit_order.side() {
            Side::Buy => {
                if self.low >= limit_order.limit_price() {
                    return None;
                }
                self.aggregate_sell_volume
                    .iter()
                    .rev()
                    .find(|v| v.0 < limit_order.limit_price())
                    .map(|v| (min(v.1, limit_order.remaining_quantity()), false))
            }
            Side::Sell => {
                if self.high <= limit_order.limit_price() {
                    return None;
                }
                self.aggregate_buy_volume
                    .iter()
                    .rev()
                    .find(|v| v.0 > limit_order.limit_price())
                    .map(|v| (min(v.1, limit_order.remaining_quantity()), false))
            }
        }
    }

    #[inline(always)]
    fn validate_market_update(
        &self,
        _price_filter: &PriceFilter<I, D>,
    ) -> Result<(), PriceFilterError> {
        // The constructor checks the validity when debug assertions are enabled.
        Ok(())
    }

    // Basically whatever the user inputs as the best bid and ask.
    #[inline]
    fn update_market_state(&self, market_state: &mut crate::prelude::MarketState<I, D>) {
        market_state.set_bid(self.bba.bid);
        market_state.set_ask(self.bba.ask);
    }

    #[inline(always)]
    fn timestamp_exchange_ns(&self) -> TimestampNs {
        self.last_timestamp_exchange_ns
    }

    #[inline(always)]
    fn can_fill_bids(&self) -> bool {
        true
    }

    #[inline(always)]
    fn can_fill_asks(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use const_decimal::Decimal;

    use super::*;
    use crate::{
        prelude::MarketState,
        types::{
            BaseCurrency,
            ExchangeOrderMeta,
            LimitOrder,
        },
        utils::NoUserOrderId,
    };

    fn mock_smart_candle() -> SmartCandle<i64, 5, BaseCurrency<i64, 5>> {
        let trades = &[Trade {
            timestamp_exchange_ns: 1.into(),
            price: QuoteCurrency::<i64, 5>::new(100, 0),
            quantity: BaseCurrency::new(1, 0),
            side: Side::Sell,
        }];
        let bba = Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 1.into(),
        };
        let pf = PriceFilter::new(
            None,
            None,
            QuoteCurrency::new(1, 0),
            Decimal::TWO,
            Decimal::try_from_scaled(5, 1).unwrap(),
        )
        .unwrap();
        SmartCandle::new(trades, bba, &pf)
    }

    #[test]
    fn smart_candle_update_market_state() {
        let smart_candle = mock_smart_candle();
        let mut state = MarketState::default();
        smart_candle.update_market_state(&mut state);
        assert_eq!(state.bid(), QuoteCurrency::new(100, 0));
        assert_eq!(state.ask(), QuoteCurrency::new(101, 0));
    }

    #[test]
    fn smart_candle_no_buys() {
        let smart_candle = mock_smart_candle();
        assert_eq!(
            smart_candle,
            SmartCandle {
                aggregate_buy_volume: Vec::new(),
                aggregate_sell_volume: vec![(QuoteCurrency::new(100, 0), BaseCurrency::new(1, 0))],
                bba: Bba {
                    bid: QuoteCurrency::new(100, 0),
                    ask: QuoteCurrency::new(101, 0),
                    timestamp_exchange_ns: 1.into(),
                },
                last_timestamp_exchange_ns: 1.into(),
                high: QuoteCurrency::new(100, 0),
                low: QuoteCurrency::new(100, 0)
            }
        );
        assert_eq!(smart_candle.timestamp_exchange_ns(), 1.into());
    }

    #[test]
    fn smart_candle_no_sells() {
        let trades = &[Trade {
            timestamp_exchange_ns: 0.into(),
            price: QuoteCurrency::<i64, 5>::new(100, 0),
            quantity: BaseCurrency::new(2, 0),
            side: Side::Buy,
        }];
        let bba = Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 0.into(),
        };
        let pf = PriceFilter::new(
            None,
            None,
            QuoteCurrency::new(1, 0),
            Decimal::TWO,
            Decimal::try_from_scaled(5, 1).unwrap(),
        )
        .unwrap();
        let smart_candle = SmartCandle::new(trades, bba, &pf);

        assert_eq!(
            smart_candle,
            SmartCandle {
                aggregate_buy_volume: vec![(QuoteCurrency::new(100, 0), BaseCurrency::new(2, 0))],
                aggregate_sell_volume: Vec::new(),
                bba,
                last_timestamp_exchange_ns: 0.into(),
                high: QuoteCurrency::new(100, 0),
                low: QuoteCurrency::new(100, 0),
            }
        )
    }

    #[test]
    fn smart_candle_simple() {
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
        let bba = Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 0.into(),
        };
        let pf = PriceFilter::new(
            None,
            None,
            QuoteCurrency::new(1, 0),
            Decimal::TWO,
            Decimal::try_from_scaled(5, 1).unwrap(),
        )
        .unwrap();
        let smart_candle = SmartCandle::new(trades, bba, &pf);

        assert_eq!(
            smart_candle,
            SmartCandle {
                aggregate_buy_volume: vec![(QuoteCurrency::new(100, 0), BaseCurrency::new(2, 0))],
                aggregate_sell_volume: vec![(QuoteCurrency::new(100, 0), BaseCurrency::new(1, 0))],
                bba,
                last_timestamp_exchange_ns: 0.into(),
                high: QuoteCurrency::new(100, 0),
                low: QuoteCurrency::new(100, 0),
            }
        )
    }

    #[test]
    fn smart_candle_sell_side() {
        let trades = &[
            Trade {
                timestamp_exchange_ns: 0.into(),
                price: QuoteCurrency::<i64, 5>::new(100, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Sell,
            },
            Trade {
                timestamp_exchange_ns: 1.into(),
                price: QuoteCurrency::<i64, 5>::new(100, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Sell,
            },
            Trade {
                timestamp_exchange_ns: 2.into(),
                price: QuoteCurrency::<i64, 5>::new(99, 0),
                quantity: BaseCurrency::new(3, 0),
                side: Side::Sell,
            },
            Trade {
                timestamp_exchange_ns: 3.into(),
                price: QuoteCurrency::<i64, 5>::new(101, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Sell,
            },
        ];
        let bba = Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 0.into(),
        };
        let pf = PriceFilter::new(
            None,
            None,
            QuoteCurrency::new(1, 0),
            Decimal::TWO,
            Decimal::try_from_scaled(5, 1).unwrap(),
        )
        .unwrap();
        let smart_candle = SmartCandle::new(trades, bba, &pf);

        assert_eq!(
            smart_candle,
            SmartCandle {
                aggregate_buy_volume: Vec::new(),
                aggregate_sell_volume: vec![
                    (QuoteCurrency::new(99, 0), BaseCurrency::new(3, 0)),
                    (QuoteCurrency::new(100, 0), BaseCurrency::new(5, 0)),
                    (QuoteCurrency::new(101, 0), BaseCurrency::new(6, 0)),
                ],
                bba,
                last_timestamp_exchange_ns: 3.into(),
                high: QuoteCurrency::new(101, 0),
                low: QuoteCurrency::new(99, 0),
            }
        )
    }

    #[test]
    fn smart_candle_buy_side() {
        let trades = &[
            Trade {
                timestamp_exchange_ns: 0.into(),
                price: QuoteCurrency::<i64, 5>::new(100, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Buy,
            },
            Trade {
                timestamp_exchange_ns: 1.into(),
                price: QuoteCurrency::<i64, 5>::new(100, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Buy,
            },
            Trade {
                timestamp_exchange_ns: 2.into(),
                price: QuoteCurrency::<i64, 5>::new(99, 0),
                quantity: BaseCurrency::new(3, 0),
                side: Side::Buy,
            },
            Trade {
                timestamp_exchange_ns: 3.into(),
                price: QuoteCurrency::<i64, 5>::new(101, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Buy,
            },
        ];
        let bba = Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 0.into(),
        };
        let pf = PriceFilter::new(
            None,
            None,
            QuoteCurrency::new(1, 0),
            Decimal::TWO,
            Decimal::try_from_scaled(5, 1).unwrap(),
        )
        .unwrap();
        let smart_candle = SmartCandle::new(trades, bba, &pf);

        assert_eq!(
            smart_candle,
            SmartCandle {
                aggregate_buy_volume: vec![
                    (QuoteCurrency::new(101, 0), BaseCurrency::new(1, 0)),
                    (QuoteCurrency::new(100, 0), BaseCurrency::new(3, 0)),
                    (QuoteCurrency::new(99, 0), BaseCurrency::new(6, 0)),
                ],
                aggregate_sell_volume: Vec::new(),
                bba,
                last_timestamp_exchange_ns: 3.into(),
                high: QuoteCurrency::new(101, 0),
                low: QuoteCurrency::new(99, 0),
            }
        )
    }

    #[test]
    fn smart_candle_execute_limit_order() {
        let trades = &[
            Trade {
                timestamp_exchange_ns: 0.into(),
                price: QuoteCurrency::<i64, 5>::new(100, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Buy,
            },
            Trade {
                timestamp_exchange_ns: 1.into(),
                price: QuoteCurrency::<i64, 5>::new(100, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Buy,
            },
            Trade {
                timestamp_exchange_ns: 2.into(),
                price: QuoteCurrency::<i64, 5>::new(99, 0),
                quantity: BaseCurrency::new(3, 0),
                side: Side::Buy,
            },
            Trade {
                timestamp_exchange_ns: 3.into(),
                price: QuoteCurrency::<i64, 5>::new(101, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Buy,
            },
            Trade {
                timestamp_exchange_ns: 3.into(),
                price: QuoteCurrency::<i64, 5>::new(102, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Buy,
            },
            Trade {
                timestamp_exchange_ns: 4.into(),
                price: QuoteCurrency::<i64, 5>::new(100, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Sell,
            },
            Trade {
                timestamp_exchange_ns: 5.into(),
                price: QuoteCurrency::<i64, 5>::new(100, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Sell,
            },
            Trade {
                timestamp_exchange_ns: 6.into(),
                price: QuoteCurrency::<i64, 5>::new(99, 0),
                quantity: BaseCurrency::new(3, 0),
                side: Side::Sell,
            },
            Trade {
                timestamp_exchange_ns: 7.into(),
                price: QuoteCurrency::<i64, 5>::new(98, 0),
                quantity: BaseCurrency::new(2, 0),
                side: Side::Sell,
            },
            Trade {
                timestamp_exchange_ns: 8.into(),
                price: QuoteCurrency::<i64, 5>::new(101, 0),
                quantity: BaseCurrency::new(1, 0),
                side: Side::Sell,
            },
        ];
        let bba = Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 0.into(),
        };
        let pf = PriceFilter::new(
            None,
            None,
            QuoteCurrency::new(1, 0),
            Decimal::TWO,
            Decimal::try_from_scaled(5, 1).unwrap(),
        )
        .unwrap();
        let mut smart_candle = SmartCandle::new(trades, bba, &pf);

        assert_eq!(
            smart_candle,
            SmartCandle {
                aggregate_buy_volume: vec![
                    (QuoteCurrency::new(102, 0), BaseCurrency::new(1, 0)),
                    (QuoteCurrency::new(101, 0), BaseCurrency::new(2, 0)),
                    (QuoteCurrency::new(100, 0), BaseCurrency::new(4, 0)),
                    (QuoteCurrency::new(99, 0), BaseCurrency::new(7, 0)),
                ],
                aggregate_sell_volume: vec![
                    (QuoteCurrency::new(98, 0), BaseCurrency::new(2, 0)),
                    (QuoteCurrency::new(99, 0), BaseCurrency::new(5, 0)),
                    (QuoteCurrency::new(100, 0), BaseCurrency::new(7, 0)),
                    (QuoteCurrency::new(101, 0), BaseCurrency::new(8, 0)),
                ],
                bba,
                last_timestamp_exchange_ns: 8.into(),
                high: QuoteCurrency::new(102, 0),
                low: QuoteCurrency::new(98, 0),
            }
        );
        let limit_buy = LimitOrder::<i64, 5, _, NoUserOrderId, _>::new(
            Side::Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(15, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let limit_order = limit_buy.into_pending(meta);
        assert_eq!(
            smart_candle.limit_order_filled(&limit_order),
            Some((BaseCurrency::new(5, 0), false))
        );

        let limit_sell = LimitOrder::<i64, 5, _, NoUserOrderId, _>::new(
            Side::Sell,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(15, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let limit_order = limit_sell.into_pending(meta);
        assert_eq!(
            smart_candle.limit_order_filled(&limit_order),
            Some((BaseCurrency::new(2, 0), false))
        );
    }

    #[test]
    fn size_of_smart_candle() {
        assert_eq!(size_of::<SmartCandle<i64, 4, BaseCurrency<i64, 4>>>(), 96);
        assert_eq!(size_of::<SmartCandle<i32, 4, BaseCurrency<i32, 4>>>(), 80);
    }
}
