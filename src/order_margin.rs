use std::num::NonZeroUsize;

use const_decimal::Decimal;
use getset::{CopyGetters, Getters, MutGetters};
use num_traits::{One, Zero};
use tracing::{debug, trace};

use crate::{
    Result,
    exchange::CancelBy,
    prelude::{ActiveLimitOrders, Currency, Mon, Position},
    types::{Balances, Error, LimitOrder, MarginCurrency, Pending, Side, UserOrderId},
    utils::max,
};

/// An implementation for computing the order margin online, aka with every change to the active orders.
#[derive(Debug, Clone, CopyGetters, Getters, MutGetters)]
pub struct OrderMargin<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    #[getset(get = "pub(crate)", get_mut = "pub(crate)")]
    active_limit_orders: ActiveLimitOrders<I, D, BaseOrQuote, UserOrderIdT>,
    bids_notional: BaseOrQuote::PairedCurrency,
    asks_notional: BaseOrQuote::PairedCurrency,
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> OrderMargin<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Create a new instance with a maximum capacity of `max_active_orders`.
    pub fn new(max_active_orders: NonZeroUsize) -> Self {
        Self {
            active_limit_orders: ActiveLimitOrders::with_capacity(max_active_orders),
            bids_notional: Zero::zero(),
            asks_notional: Zero::zero(),
        }
    }

    /// Insert a new limit order.
    #[inline(always)]
    pub fn try_insert(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
        position: &Position<I, D, BaseOrQuote>,
        init_margin_req: Decimal<I, D>,
    ) -> Result<()> {
        trace!("OrderMargin.try_insert {order:?}");
        self.active_limit_orders.try_insert(order.clone())?;
        match order.side() {
            Side::Buy => self.bids_notional += order.notional(),
            Side::Sell => self.asks_notional += order.notional(),
        }

        // Update balances
        let new_order_margin = self.order_margin(init_margin_req, position);
        assert2::debug_assert!(new_order_margin >= balances.order_margin());
        if new_order_margin > balances.order_margin() {
            let margin = new_order_margin - balances.order_margin();
            assert2::debug_assert!(margin >= BaseOrQuote::PairedCurrency::zero());
            let success = balances.try_reserve_order_margin(margin);
            debug_assert!(success, "Can place order");
        }

        Ok(())
    }

    /// fill an existing limit order, reduces order margin.
    /// # Panics:
    /// panics if the order id was not found.
    pub fn fill_order(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
        position: &Position<I, D, BaseOrQuote>,
        init_margin_req: Decimal<I, D>,
    ) {
        trace!("OrderMargin.update {order:?}");
        let notional = order.notional();
        assert2::debug_assert!(notional > Zero::zero());
        let old_order = self.active_limit_orders.update(order);
        let notional_delta = notional - old_order.notional();
        match old_order.side() {
            Side::Buy => {
                self.bids_notional += notional_delta;
                assert2::debug_assert!(self.bids_notional >= Zero::zero());
            }
            Side::Sell => {
                self.asks_notional += notional_delta;
                assert2::debug_assert!(self.asks_notional >= Zero::zero());
            }
        }

        // Update balances
        let new_order_margin = self.order_margin(init_margin_req, position);
        assert2::debug_assert!(
            new_order_margin <= balances.order_margin(),
            "The order margin does not increase with a filled limit order event."
        );
        if new_order_margin < balances.order_margin() {
            let margin_delta = balances.order_margin() - new_order_margin;
            assert2::debug_assert!(margin_delta > Zero::zero());
            balances.free_order_margin(margin_delta);
        }
    }

    /// Remove an order from being tracked for margin purposes.
    pub fn remove(
        &mut self,
        by: CancelBy<UserOrderIdT>,
        balances: &mut Balances<I, D, BaseOrQuote::PairedCurrency>,
        position: &Position<I, D, BaseOrQuote>,
        init_margin_req: Decimal<I, D>,
    ) -> Result<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        debug!("OrderMargin.remove {by:?}");
        let removed_order = match by {
            CancelBy::OrderId(order_id) => self
                .active_limit_orders
                .remove(order_id)
                .ok_or(Error::OrderIdNotFound { order_id })?,
            CancelBy::UserOrderId(user_order_id) => self
                .active_limit_orders
                .remove_by_user_order_id(user_order_id)
                .ok_or(Error::UserOrderIdNotFound)?,
        };

        match removed_order.side() {
            Side::Buy => {
                self.bids_notional -= removed_order.notional();
                assert2::debug_assert!(self.bids_notional >= Zero::zero());
            }
            Side::Sell => {
                self.asks_notional -= removed_order.notional();
                assert2::debug_assert!(self.asks_notional >= Zero::zero());
            }
        }

        // Update balances
        let new_order_margin = self.order_margin(init_margin_req, position);
        assert2::debug_assert!(
            new_order_margin <= balances.order_margin(),
            "When removing a limit order, the new order margin is smaller or equal the old order margin"
        );
        if new_order_margin < balances.order_margin() {
            let margin = balances.order_margin() - new_order_margin;
            assert2::debug_assert!(margin >= Zero::zero());
            balances.free_order_margin(margin);
        }

        Ok(removed_order)
    }

    /// The margin requirement for all the tracked orders.
    pub fn order_margin(
        &self,
        init_margin_req: Decimal<I, D>,
        position: &Position<I, D, BaseOrQuote>,
    ) -> BaseOrQuote::PairedCurrency {
        assert2::debug_assert!(init_margin_req > Decimal::zero());
        assert2::debug_assert!(init_margin_req <= Decimal::one());

        match position {
            Position::Neutral => max(self.bids_notional, self.asks_notional) * init_margin_req,
            Position::Long(inner) => {
                max(self.bids_notional, self.asks_notional - inner.notional()) * init_margin_req
            }
            Position::Short(inner) => {
                max(self.bids_notional - inner.notional(), self.asks_notional) * init_margin_req
            }
        }
    }

    /// Get the order margin if a new order were to be added.
    pub(crate) fn order_margin_with_order(
        &self,
        new_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        init_margin_req: Decimal<I, D>,
        position: &Position<I, D, BaseOrQuote>,
    ) -> BaseOrQuote::PairedCurrency {
        assert2::debug_assert!(init_margin_req > Decimal::zero());
        assert2::debug_assert!(init_margin_req <= Decimal::one());

        let mut buy_notional = self.bids_notional;
        let mut sell_notional = self.asks_notional;
        let new_notional = new_order.notional();
        match new_order.side() {
            Side::Buy => buy_notional += new_notional,
            Side::Sell => sell_notional += new_notional,
        }

        match position {
            Position::Neutral => max(buy_notional, sell_notional) * init_margin_req,
            Position::Long(inner) => {
                let notional = inner.notional();
                trace!("notional: {notional}");
                max(buy_notional, sell_notional - notional) * init_margin_req
            }
            Position::Short(inner) => {
                let notional = inner.notional();
                trace!("notional: {notional}");
                max(buy_notional - notional, sell_notional) * init_margin_req
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DECIMALS, prelude::*, test_fee_maker};

    #[test]
    fn order_margin_assert_limit_order_reduces_qty() {
        let new_active_order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 1.into());
        let active_order = new_active_order.into_pending(meta);

        let mut updated_order = active_order.clone();
        let fee = QuoteCurrency::new(0, 0);
        updated_order.fill(BaseCurrency::new(1, 0), fee, 1.into());

        ActiveLimitOrders::assert_limit_order_update_reduces_qty(&active_order, &updated_order);
    }

    #[test]
    #[should_panic]
    fn order_margin_assert_limit_order_reduces_qty_panic() {
        let new_active_order = LimitOrder::new(
            Side::Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
        )
        .unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 1.into());
        let order_0 = new_active_order.into_pending(meta);

        let mut order_1 = order_0.clone();
        let fee = QuoteCurrency::new(0, 0);
        order_1.fill(BaseCurrency::new(1, 0), fee, 1.into());

        ActiveLimitOrders::assert_limit_order_update_reduces_qty(&order_1, &order_0);
    }

    #[test_case::test_matrix(
        [1, 2, 5]
    )]
    #[tracing_test::traced_test]
    fn order_margin_neutral_no_orders(leverage: u8) {
        let order_margin =
            OrderMargin::<_, 4, _, NoUserOrderId>::new(NonZeroUsize::new(10).unwrap());

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let position = Position::<_, 4, BaseCurrency<i32, 4>>::Neutral;
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [1, 2, 5],
        [100, 200, 300]
    )]
    fn order_margin_long_no_orders(leverage: u8, position_qty: i32, entry_price: i32) {
        let order_margin =
            OrderMargin::<_, 4, _, NoUserOrderId>::new(NonZeroUsize::new(10).unwrap());

        let qty = BaseCurrency::new(position_qty, 0);
        let entry_price = QuoteCurrency::new(entry_price, 0);
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let mut balances = Balances::new(QuoteCurrency::new(1500, 0));
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let position = Position::Long(PositionInner::new(qty, entry_price));

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [1, 2, 5],
        [100, 200, 300]
    )]
    fn order_margin_short_no_orders(leverage: u8, position_qty: i32, entry_price: i32) {
        let order_margin =
            OrderMargin::<_, 4, _, NoUserOrderId>::new(NonZeroUsize::new(10).unwrap());

        let qty = BaseCurrency::new(position_qty, 0);
        let entry_price = QuoteCurrency::new(entry_price, 0);
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let mut balances = Balances::new(QuoteCurrency::new(10000, 0));
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let position = Position::Short(PositionInner::new(qty, entry_price));

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy, Side::Sell],
        [100, 150, 200],
        [1, 2, 3],
        [1, 2, 3]
    )]
    fn order_margin_neutral_orders_of_same_side(
        leverage: u8,
        side: Side,
        limit_price: i32,
        qty: i32,
        n: usize,
    ) {
        let mut order_margin =
            OrderMargin::<_, 4, _, NoUserOrderId>::new(NonZeroUsize::new(10).unwrap());

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta =
                ExchangeOrderMeta::new((i as u64).into(), Into::<TimestampNs>::into(i as i64));
            order.into_pending(meta)
        }));
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        let position = Position::Neutral;
        orders.iter().for_each(|order| {
            order_margin
                .try_insert(order.clone(), &mut balances, &position, init_margin_req)
                .unwrap()
        });

        let mult = QuoteCurrency::new(n as _, 0);
        let om = mult * QuoteCurrency::convert_from(qty, limit_price) * init_margin_req;
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            om
        );
        assert_eq!(balances.order_margin(), om);

        orders.iter().for_each(|order| {
            let _ = order_margin.remove(
                CancelBy::OrderId(order.id()),
                &mut balances,
                &position,
                init_margin_req,
            );
        });
        let om = QuoteCurrency::new(0, 0);
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            om
        );
        assert_eq!(balances.order_margin(), om);
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy],
        [100, 150, 200],
        [1, 2, 3],
        [1, 2, 3]
    )]
    fn order_margin_neutral_orders_of_opposite_side(
        leverage: u8,
        side: Side,
        limit_price: i32,
        qty: i32,
        n: usize,
    ) {
        let mut order_margin =
            OrderMargin::<_, 4, _, NoUserOrderId>::new(NonZeroUsize::new(10).unwrap());

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let buy_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
            order.into_pending(meta)
        }));
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        let position = Position::Neutral;
        buy_orders.iter().for_each(|order| {
            order_margin
                .try_insert(order.clone(), &mut balances, &position, init_margin_req)
                .unwrap();
        });
        let notional: QuoteCurrency<i32, 4> = buy_orders.iter().map(|o| o.notional()).sum();
        assert_eq!(balances.order_margin(), notional * init_margin_req);

        let sell_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side.inverted(), limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new(((n + i) as u64).into(), ((n + i) as i64).into());
            order.into_pending(meta)
        }));
        sell_orders.iter().for_each(|order| {
            order_margin
                .try_insert(order.clone(), &mut balances, &position, init_margin_req)
                .unwrap();
        });

        let mult = QuoteCurrency::new(n as _, 0);
        let om = mult * QuoteCurrency::convert_from(qty, limit_price) * init_margin_req;
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral,
            ),
            om
        );
        assert_eq!(balances.order_margin(), om);

        buy_orders.iter().for_each(|order| {
            let _ = order_margin.remove(
                CancelBy::OrderId(order.id()),
                &mut balances,
                &position,
                init_margin_req,
            );
        });
        sell_orders.iter().for_each(|order| {
            let _ = order_margin.remove(
                CancelBy::OrderId(order.id()),
                &mut balances,
                &position,
                init_margin_req,
            );
        });
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            QuoteCurrency::new(0, 0)
        );
        assert_eq!(balances.order_margin(), Zero::zero());
    }

    #[test_case::test_matrix(
        [1, 2, 5]
    )]
    fn order_margin_long_orders_of_same_qty(leverage: u8) {
        let mut order_margin =
            OrderMargin::<_, 4, _, NoUserOrderId>::new(NonZeroUsize::new(10).unwrap());
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(3, 0);
        let limit_price = QuoteCurrency::new(100, 0);
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        let position = Position::Neutral;

        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut balances, &position, init_margin_req)
            .unwrap();

        let pos_entry_price = QuoteCurrency::new(90, 0);
        let position = Position::Short(PositionInner::new(qty, pos_entry_price));

        // The limit orders may require more margin.
        let om = QuoteCurrency::convert_from(qty, QuoteCurrency::new(10, 0)) * init_margin_req;

        assert_eq!(order_margin.order_margin(init_margin_req, &position), om);
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Side::Buy, Side::Sell],
        [70, 90, 110],
        [1, 2, 3]
    )]
    #[tracing_test::traced_test]
    fn order_margin_neutral_update_partial_fills(
        leverage: u8,
        side: Side,
        limit_price: i64,
        qty: i64,
    ) {
        let mut order_margin =
            OrderMargin::<_, DECIMALS, _, NoUserOrderId>::new(NonZeroUsize::new(10).unwrap());
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let mut order = order.into_pending(meta);
        let notional = order.notional();
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        let position = Position::Neutral;
        order_margin
            .try_insert(order.clone(), &mut balances, &position, init_margin_req)
            .unwrap();
        assert_eq!(order_margin.active_limit_orders.num_active(), 1);
        assert_eq!(balances.order_margin(), notional * init_margin_req);

        // Now partially fill the order
        let filled_qty = qty / BaseCurrency::new(2, 0);
        let fee = QuoteCurrency::convert_from(filled_qty, limit_price) * *test_fee_maker().as_ref();
        match order.fill(filled_qty, fee, 0.into()) {
            LimitOrderFill::PartiallyFilled {
                filled_quantity,
                fee: f,
                order_after_fill: _,
            } => {
                assert_eq!(filled_quantity, filled_qty);
                assert_eq!(f, fee);
            }
            LimitOrderFill::FullyFilled { .. } => panic!("Expected `PartiallyFilled`"),
        }
        let remaining_qty = order.remaining_quantity();
        order_margin.fill_order(order, &mut balances, &position, init_margin_req);
        assert_eq!(order_margin.active_limit_orders.num_active(), 1);
        assert_eq!(remaining_qty, filled_qty);
        let om = QuoteCurrency::convert_from(remaining_qty, limit_price) * init_margin_req;
        assert_eq!(
            order_margin.order_margin(init_margin_req, &Position::Neutral),
            om,
        );
        assert_eq!(balances.order_margin(), om);
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_no_position() {
        let position = Position::default();
        let init_margin_req = Decimal::one();
        let mut order_margin = OrderMargin::new(NonZeroUsize::new(10).unwrap());

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let qty = BaseCurrency::<i32, 4>::new(1, 0);
        let limit_price = QuoteCurrency::new(90, 0);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        let position = Position::Neutral;
        order_margin
            .try_insert(order, &mut balances, &position, init_margin_req)
            .unwrap();
        assert_eq!(order_margin.active_limit_orders.asks().len(), 0);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(90, 0)
        );

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut balances, &position, init_margin_req)
            .unwrap();
        assert_eq!(order_margin.active_limit_orders.bids().len(), 1);
        assert_eq!(order_margin.active_limit_orders.asks().len(), 1);
        let om = QuoteCurrency::new(100, 0);
        assert_eq!(order_margin.order_margin(init_margin_req, &position), om);
        assert_eq!(balances.order_margin(), om);

        let limit_price = QuoteCurrency::new(120, 0);
        let qty = BaseCurrency::new(1, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut balances, &position, init_margin_req)
            .unwrap();
        assert_eq!(order_margin.active_limit_orders.bids().len(), 1);
        assert_eq!(order_margin.active_limit_orders.asks().len(), 2);
        let om = QuoteCurrency::new(220, 0);
        assert_eq!(order_margin.order_margin(init_margin_req, &position), om);
        assert_eq!(balances.order_margin(), om);
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_long() {
        let mut order_margin = OrderMargin::new(NonZeroUsize::new(10).unwrap());

        let qty = BaseCurrency::<i64, 5>::new(1, 0);
        let entry_price = QuoteCurrency::new(100, 0);

        let position = Position::Long(PositionInner::new(qty, entry_price));
        let init_margin_req = Decimal::one();

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let limit_price = QuoteCurrency::new(90, 0);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        order_margin
            .try_insert(order, &mut balances, &position, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(90, 0);
        assert_eq!(order_margin.order_margin(init_margin_req, &position), om);
        assert_eq!(balances.order_margin(), om);

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut balances, &position, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(90, 0);
        assert_eq!(order_margin.order_margin(init_margin_req, &position), om,);
        assert_eq!(balances.order_margin(), om);

        let limit_price = QuoteCurrency::new(120, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut balances, &position, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(120, 0);
        assert_eq!(order_margin.order_margin(init_margin_req, &position), om);
        assert_eq!(balances.order_margin(), om);

        let limit_price = QuoteCurrency::new(95, 0);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut balances, &position, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(185, 0);
        assert_eq!(order_margin.order_margin(init_margin_req, &position), om);
        assert_eq!(balances.order_margin(), om);
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_short() {
        let mut order_margin =
            OrderMargin::<i64, 5, _, NoUserOrderId>::new(NonZeroUsize::new(10).unwrap());

        let qty = BaseCurrency::<i64, DECIMALS>::one();
        let entry_price = QuoteCurrency::new(100, 0);

        let position = Position::Short(PositionInner::new(qty, entry_price));
        let init_margin_req = Decimal::one();

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let limit_price = QuoteCurrency::new(90, 0);
        let qty = BaseCurrency::one();
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        let mut balances = Balances::new(QuoteCurrency::new(10_000, 0));
        order_margin
            .try_insert(order, &mut balances, &position, init_margin_req)
            .unwrap();
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::zero()
        );
        assert_eq!(balances.order_margin(), Zero::zero());

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut balances, &position, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(100, 0);
        assert_eq!(order_margin.order_margin(init_margin_req, &position), om);
        assert_eq!(balances.order_margin(), om);

        let limit_price = QuoteCurrency::new(120, 0);
        let order = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut balances, &position, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(220, 0);
        assert_eq!(order_margin.order_margin(init_margin_req, &position), om);
        assert_eq!(balances.order_margin(), om);

        let limit_price = QuoteCurrency::new(95, 0);
        let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut balances, &position, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(220, 0);
        assert_eq!(order_margin.order_margin(init_margin_req, &position), om);
        assert_eq!(balances.order_margin(), om);
    }
}
