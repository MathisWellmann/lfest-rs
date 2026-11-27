use getset::{
    CopyGetters,
    Getters,
};
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};

use super::{
    Bba,
    MarketUpdate,
    Trade,
};
use crate::{
    EXPECT_CAPACITY,
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
    utils::{
        max,
        min,
    },
};

/// A data structure for aggregated trades with the ability to approximate realistic taker fill flow.
/// Basically a `Candle` buy one that does not blindly fill active limit orders with taker flow that does not exist.
#[derive(Debug, Clone, Eq, PartialEq, Getters, CopyGetters)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
            let flow = match trade.side {
                Side::Buy => &mut buys,
                Side::Sell => &mut sells,
            };
            flow.push_within_capacity((trade.price, trade.quantity))
                .expect(EXPECT_CAPACITY);
            low = min(low, trade.price);
            high = max(high, trade.price);
        }

        // Taker buys fill resting sells priced below them, so the most aggressive (highest)
        // buy comes first; for taker sells the lowest comes first.
        glidesort::sort_by_key(&mut buys, |t| -t.0);
        glidesort::sort_by_key(&mut sells, |t| t.0);

        Self {
            high,
            low,
            aggregate_buy_volume: Self::into_cumulative_levels(buys),
            aggregate_sell_volume: Self::into_cumulative_levels(sells),
            last_timestamp_exchange_ns: taker_trades[taker_trades.len() - 1].timestamp_exchange_ns,
            bba,
        }
    }

    /// Merge the sorted `(price, quantity)` flow into one level per price, holding the
    /// quantity of that level and of all levels before it. Reuses the allocation.
    fn into_cumulative_levels(
        mut levels: Vec<(QuoteCurrency<I, D>, BaseOrQuote)>,
    ) -> Vec<(QuoteCurrency<I, D>, BaseOrQuote)> {
        levels.dedup_by(|next, kept| {
            let same_price = next.0 == kept.0;
            if same_price {
                kept.1 += next.1;
            }
            same_price
        });
        let mut cumulative = BaseOrQuote::zero();
        for level in &mut levels {
            cumulative += level.1;
            level.1 = cumulative;
        }
        levels.shrink_to_fit();
        debug_assert!(levels.windows(2).all(|w| w[0].0 != w[1].0));
        debug_assert!(levels.windows(2).all(|w| w[0].1 <= w[1].1));
        levels
    }

    /// Decrement `consumed` volume from a cumulative level array while preserving
    /// monotonicity across all price levels.
    ///
    /// Levels at indices `0..=pos` hold cumulative volume `<= avail`; consuming
    /// `consumed` reduces `levels[pos..]` by `consumed` (consuming from the matched
    /// bucket up through outer cumulative sums) and clamps `levels[0..pos]` to at
    /// most the new `levels[pos]` so deeper levels never exceed shallower ones.
    #[inline]
    fn deplete_cumulative_levels(
        levels: &mut [(QuoteCurrency<I, D>, BaseOrQuote)],
        pos: usize,
        consumed: BaseOrQuote,
    ) {
        assert2::debug_assert!(consumed > BaseOrQuote::zero());
        assert2::debug_assert!(consumed <= levels[pos].1);
        // The levels are non-decreasing, so none of `levels[pos..]` drops below zero.
        for level in &mut levels[pos..] {
            level.1 -= consumed;
        }
        let cap_after = levels[pos].1;
        for level in &mut levels[..pos] {
            level.1 = min(level.1, cap_after);
        }
        debug_assert!(levels.windows(2).all(|w| w[0].1 <= w[1].1));
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
        let limit = limit_order.limit_price();
        // Taker flow fills a resting order when it trades through the limit price; the
        // cumulative level just inside the limit holds all the flow that did.
        let (levels, pos) = match limit_order.side() {
            Side::Buy => {
                let pos = self
                    .aggregate_sell_volume
                    .iter()
                    .rposition(|v| v.0 < limit)?;
                (&mut self.aggregate_sell_volume, pos)
            }
            Side::Sell => {
                let pos = self
                    .aggregate_buy_volume
                    .iter()
                    .rposition(|v| v.0 > limit)?;
                (&mut self.aggregate_buy_volume, pos)
            }
        };
        let avail = levels[pos].1;
        if avail <= BaseOrQuote::zero() {
            return None;
        }
        let filled = min(avail, limit_order.remaining_quantity());
        Self::deplete_cumulative_levels(levels, pos, filled);
        let exhausted = levels.last().is_none_or(|v| v.1 <= BaseOrQuote::zero());
        Some((filled, exhausted))
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
    fn price_range(&self) -> Option<(QuoteCurrency<I, D>, QuoteCurrency<I, D>)> {
        Some((self.low, self.high))
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

    /// The candle of `smart_candle_execute_limit_order`: sell levels
    /// `[(98, 2), (99, 5), (100, 7), (101, 8)]`, buy levels
    /// `[(102, 1), (101, 2), (100, 4), (99, 7)]`.
    fn mixed_flow_candle() -> SmartCandle<i64, 5, BaseCurrency<i64, 5>> {
        let trade = |ts: i64, price: i64, qty: i64, side: Side| Trade {
            timestamp_exchange_ns: ts.into(),
            price: QuoteCurrency::<i64, 5>::new(price, 0),
            quantity: BaseCurrency::new(qty, 0),
            side,
        };
        let trades = [
            trade(0, 100, 1, Side::Buy),
            trade(1, 100, 1, Side::Buy),
            trade(2, 99, 3, Side::Buy),
            trade(3, 101, 1, Side::Buy),
            trade(3, 102, 1, Side::Buy),
            trade(4, 100, 1, Side::Sell),
            trade(5, 100, 1, Side::Sell),
            trade(6, 99, 3, Side::Sell),
            trade(7, 98, 2, Side::Sell),
            trade(8, 101, 1, Side::Sell),
        ];
        let bba = Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 0.into(),
        };
        SmartCandle::new(&trades, bba, &PriceFilter::default())
    }

    fn pending(
        side: Side,
        price: i64,
        qty: i64,
    ) -> LimitOrder<
        i64,
        5,
        BaseCurrency<i64, 5>,
        NoUserOrderId,
        crate::prelude::Pending<i64, 5, BaseCurrency<i64, 5>>,
    > {
        LimitOrder::new(
            side,
            QuoteCurrency::new(price, 0),
            BaseCurrency::new(qty, 0),
        )
        .unwrap()
        .into_pending(ExchangeOrderMeta::new(0.into(), 0.into()))
    }

    #[test]
    fn smart_candle_successive_buys_share_the_depleted_sell_flow() {
        let mut candle = mixed_flow_candle();
        // Sells at 98 and 99 (5 in total) traded through a buy at 100.
        assert_eq!(
            candle.limit_order_filled(&pending(Side::Buy, 100, 3)),
            Some((BaseCurrency::new(3, 0), false))
        );
        assert_eq!(
            candle.aggregate_sell_volume,
            vec![
                (QuoteCurrency::new(98, 0), BaseCurrency::new(2, 0)),
                (QuoteCurrency::new(99, 0), BaseCurrency::new(2, 0)),
                (QuoteCurrency::new(100, 0), BaseCurrency::new(4, 0)),
                (QuoteCurrency::new(101, 0), BaseCurrency::new(5, 0)),
            ]
        );
        // Only 2 of the sell flow below 100 are left for a second buy at 100 ..
        assert_eq!(
            candle.limit_order_filled(&pending(Side::Buy, 100, 10)),
            Some((BaseCurrency::new(2, 0), false))
        );
        // .. and the rest of the sell flow then exhausts it.
        assert_eq!(
            candle.limit_order_filled(&pending(Side::Buy, 102, 10)),
            Some((BaseCurrency::new(3, 0), true))
        );
        assert_eq!(candle.limit_order_filled(&pending(Side::Buy, 102, 1)), None);
        // The buy flow is untouched.
        assert_eq!(
            candle.limit_order_filled(&pending(Side::Sell, 98, 10)),
            Some((BaseCurrency::new(7, 0), true))
        );
    }

    #[test]
    fn smart_candle_does_not_fill_at_the_traded_price() {
        let mut candle = mixed_flow_candle();
        // No sell traded below 98 and no buy above 102.
        assert_eq!(candle.limit_order_filled(&pending(Side::Buy, 98, 1)), None);
        assert_eq!(
            candle.limit_order_filled(&pending(Side::Sell, 102, 1)),
            None
        );
        assert_eq!(
            candle.limit_order_filled(&pending(Side::Sell, 101, 5)),
            Some((BaseCurrency::new(1, 0), false))
        );
    }

    #[test]
    fn smart_candle_reports_its_range_and_time() {
        let candle = mixed_flow_candle();
        assert_eq!(
            candle.price_range(),
            Some((QuoteCurrency::new(98, 0), QuoteCurrency::new(102, 0)))
        );
        assert_eq!(candle.timestamp_exchange_ns(), 8.into());
        assert_eq!(
            candle.to_string(),
            "SmartCandle with 4 buy volume levels and 4 sell volume levels. last_timestamp_exchange_ns: 8"
        );
    }

    #[test]
    fn size_of_smart_candle() {
        assert_eq!(size_of::<SmartCandle<i64, 4, BaseCurrency<i64, 4>>>(), 96);
        assert_eq!(size_of::<SmartCandle<i32, 4, BaseCurrency<i32, 4>>>(), 80);
    }
}
