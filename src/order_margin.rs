// TODO: migrate tests.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DECIMALS,
        prelude::*,
        test_fee_maker,
    };

    #[test]
    fn order_margin_assert_limit_order_reduces_qty() {
        let new_active_order = LimitOrder::new(
            Buy,
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
            Buy,
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
        let order_margin = OrderMargin::<_, 4, _, NoUserOrderId>::new(NonZeroU16::new(10).unwrap());

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
        let order_margin = OrderMargin::<_, 4, _, NoUserOrderId>::new(NonZeroU16::new(10).unwrap());

        let qty = BaseCurrency::new(position_qty, 0);
        let entry_price = QuoteCurrency::new(entry_price, 0);
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let mut balances = Balances::new(QuoteCurrency::new(1500, 0));
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let position = Long(PositionInner::new(qty, entry_price));

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
        let order_margin = OrderMargin::<_, 4, _, NoUserOrderId>::new(NonZeroU16::new(10).unwrap());

        let qty = BaseCurrency::new(position_qty, 0);
        let entry_price = QuoteCurrency::new(entry_price, 0);
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let mut balances = Balances::new(QuoteCurrency::new(10000, 0));
        let notional = QuoteCurrency::convert_from(qty, entry_price);
        let init_margin = notional * init_margin_req;
        assert!(balances.try_reserve_order_margin(init_margin));

        let position = Short(PositionInner::new(qty, entry_price));

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Buy, Sell],
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
            OrderMargin::<_, 4, _, NoUserOrderId>::new(NonZeroU16::new(10).unwrap());

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta =
                ExchangeOrderMeta::new((i as u64).into(), Into::<TimestampNs>::into(i as i64));
            order.into_pending(meta)
        }));
        let mut account = Account::builder()
            .balances(Balances::new(QuoteCurrency::new(10_000, 0)))
            .build();
        orders.iter().for_each(|order| {
            order_margin
                .try_insert(order.clone(), &mut account, init_margin_req)
                .unwrap()
        });

        let mult = QuoteCurrency::new(n as i32, 0);
        let om = mult * QuoteCurrency::convert_from(qty, limit_price) * init_margin_req;
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            om
        );
        assert_eq!(account.balances().order_margin(), om);

        orders.iter().for_each(|order| {
            let _ =
                order_margin.remove(CancelBy::OrderId(order.id()), &mut account, init_margin_req);
        });
        let om = QuoteCurrency::new(0, 0);
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            om
        );
        assert_eq!(account.balances().order_margin(), om);
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Buy],
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
            OrderMargin::<_, 4, _, NoUserOrderId>::new(NonZeroU16::new(10).unwrap());

        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();

        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let buy_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side, limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new((i as u64).into(), (i as i64).into());
            order.into_pending(meta)
        }));
        let mut account = Account::builder()
            .balances(Balances::new(QuoteCurrency::new(10_000, 0)))
            .build();
        buy_orders.iter().for_each(|order| {
            order_margin
                .try_insert(order.clone(), &mut account, init_margin_req)
                .unwrap();
        });
        let notional: QuoteCurrency<i32, 4> = buy_orders.iter().map(|o| o.notional()).sum();
        assert_eq!(
            account.balances().order_margin(),
            notional * init_margin_req
        );

        let sell_orders = Vec::from_iter((0..n).map(|i| {
            let order = LimitOrder::new(side.inverted(), limit_price, qty).unwrap();
            let meta = ExchangeOrderMeta::new(((n + i) as u64).into(), ((n + i) as i64).into());
            order.into_pending(meta)
        }));
        sell_orders.iter().for_each(|order| {
            order_margin
                .try_insert(order.clone(), &mut account, init_margin_req)
                .unwrap();
        });

        let mult = QuoteCurrency::new(n as i32, 0);
        let om = mult * QuoteCurrency::convert_from(qty, limit_price) * init_margin_req;
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral,
            ),
            om
        );
        assert_eq!(account.balances().order_margin(), om);

        buy_orders.iter().for_each(|order| {
            let _ =
                order_margin.remove(CancelBy::OrderId(order.id()), &mut account, init_margin_req);
        });
        sell_orders.iter().for_each(|order| {
            let _ =
                order_margin.remove(CancelBy::OrderId(order.id()), &mut account, init_margin_req);
        });
        assert_eq!(
            order_margin.order_margin(
                init_margin_req,
                &Position::<_, 4, BaseCurrency<i32, 4>>::Neutral
            ),
            QuoteCurrency::new(0, 0)
        );
        assert_eq!(account.balances().order_margin(), Zero::zero());
    }

    #[test_case::test_matrix(
        [1, 2, 5]
    )]
    fn order_margin_long_orders_of_same_qty(leverage: u8) {
        let mut order_margin =
            OrderMargin::<_, 4, _, NoUserOrderId>::new(NonZeroU16::new(10).unwrap());
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(3, 0);
        let limit_price = QuoteCurrency::new(100, 0);
        let mut account = Account::builder()
            .balances(Balances::new(QuoteCurrency::new(10_000, 0)))
            .build();

        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut account, init_margin_req)
            .unwrap();

        let pos_entry_price = QuoteCurrency::new(90, 0);
        let position = Short(PositionInner::new(qty, pos_entry_price));

        // The limit orders may require more margin.
        let om = QuoteCurrency::convert_from(qty, QuoteCurrency::new(10, 0)) * init_margin_req;

        assert_eq!(order_margin.order_margin(init_margin_req, &position), om);
    }

    #[test_case::test_matrix(
        [1, 2, 5],
        [Buy, Sell],
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
            OrderMargin::<_, DECIMALS, _, NoUserOrderId>::new(NonZeroU16::new(10).unwrap());
        let init_margin_req = Leverage::new(leverage).unwrap().init_margin_req();
        let qty = BaseCurrency::new(qty, 0);
        let limit_price = QuoteCurrency::new(limit_price, 0);

        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let mut order = order.into_pending(meta);
        let notional = order.notional();
        let mut account = Account::builder()
            .balances(Balances::new(QuoteCurrency::new(10_000, 0)))
            .build();
        order_margin
            .try_insert(order.clone(), &mut account, init_margin_req)
            .unwrap();
        assert_eq!(order_margin.active_limit_orders.num_active(), 1);
        assert_eq!(
            account.balances().order_margin(),
            notional * init_margin_req
        );

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
        order_margin.fill_order(&order, &mut account, init_margin_req);
        assert_eq!(order_margin.active_limit_orders.num_active(), 1);
        assert_eq!(remaining_qty, filled_qty);
        let om = QuoteCurrency::convert_from(remaining_qty, limit_price) * init_margin_req;
        assert_eq!(order_margin.order_margin(init_margin_req, &Neutral), om,);
        assert_eq!(account.balances().order_margin(), om);
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_no_position() {
        let position = Position::default();
        let init_margin_req = Decimal::one();
        let mut order_margin = OrderMargin::new(NonZeroU16::new(10).unwrap());

        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(0, 0)
        );

        let qty = BaseCurrency::<i32, 4>::new(1, 0);
        let limit_price = QuoteCurrency::new(90, 0);
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        let mut account = Account::builder()
            .balances(Balances::new(QuoteCurrency::new(10_000, 0)))
            .build();
        order_margin
            .try_insert(order, &mut account, init_margin_req)
            .unwrap();
        assert_eq!(order_margin.active_limit_orders.asks().len(), 0);
        assert_eq!(
            order_margin.order_margin(init_margin_req, &position),
            QuoteCurrency::new(90, 0)
        );

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut account, init_margin_req)
            .unwrap();
        assert_eq!(order_margin.active_limit_orders.bids().len(), 1);
        assert_eq!(order_margin.active_limit_orders.asks().len(), 1);
        let om = QuoteCurrency::new(100, 0);
        assert_eq!(order_margin.order_margin(init_margin_req, &position), om);
        assert_eq!(account.balances().order_margin(), om);

        let limit_price = QuoteCurrency::new(120, 0);
        let qty = BaseCurrency::new(1, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut account, init_margin_req)
            .unwrap();
        assert_eq!(order_margin.active_limit_orders.bids().len(), 1);
        assert_eq!(order_margin.active_limit_orders.asks().len(), 2);
        let om = QuoteCurrency::new(220, 0);
        assert_eq!(order_margin.order_margin(init_margin_req, &position), om);
        assert_eq!(account.balances().order_margin(), om);
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_long() {
        let mut order_margin = OrderMargin::new(NonZeroU16::new(10).unwrap());

        let qty = BaseCurrency::<i64, 5>::new(1, 0);
        let entry_price = QuoteCurrency::new(100, 0);

        let mut account = Account::builder()
            .balances(Balances::new(QuoteCurrency::new(10_000, 0)))
            .position(Long(PositionInner::new(qty, entry_price)))
            .build();
        let init_margin_req = Decimal::one();

        assert_eq!(
            order_margin.order_margin(init_margin_req, account.position()),
            QuoteCurrency::new(0, 0)
        );

        let limit_price = QuoteCurrency::new(90, 0);
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut account, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(90, 0);
        assert_eq!(
            order_margin.order_margin(init_margin_req, account.position()),
            om
        );
        assert_eq!(account.balances().order_margin(), om);

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut account, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(90, 0);
        assert_eq!(
            order_margin.order_margin(init_margin_req, account.position()),
            om,
        );
        assert_eq!(account.balances().order_margin(), om);

        let limit_price = QuoteCurrency::new(120, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut account, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(120, 0);
        assert_eq!(
            order_margin.order_margin(init_margin_req, account.position()),
            om
        );
        assert_eq!(account.balances().order_margin(), om);

        let limit_price = QuoteCurrency::new(95, 0);
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut account, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(185, 0);
        assert_eq!(
            order_margin.order_margin(init_margin_req, account.position()),
            om
        );
        assert_eq!(account.balances().order_margin(), om);
    }

    #[test]
    #[tracing_test::traced_test]
    fn order_margin_with_short() {
        let mut order_margin =
            OrderMargin::<i64, 5, _, NoUserOrderId>::new(NonZeroU16::new(10).unwrap());

        let qty = BaseCurrency::<i64, DECIMALS>::one();
        let entry_price = QuoteCurrency::new(100, 0);

        let mut account = Account::builder()
            .balances(Balances::new(QuoteCurrency::new(10_000, 0)))
            .position(Short(PositionInner::new(qty, entry_price)))
            .build();
        let init_margin_req = Decimal::one();

        assert_eq!(
            order_margin.order_margin(init_margin_req, account.position()),
            QuoteCurrency::new(0, 0)
        );

        let limit_price = QuoteCurrency::new(90, 0);
        let qty = BaseCurrency::one();
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut account, init_margin_req)
            .unwrap();
        assert_eq!(
            order_margin.order_margin(init_margin_req, account.position()),
            QuoteCurrency::zero()
        );
        assert_eq!(account.balances().order_margin(), Zero::zero());

        let limit_price = QuoteCurrency::new(100, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(1.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut account, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(100, 0);
        assert_eq!(
            order_margin.order_margin(init_margin_req, account.position()),
            om
        );
        assert_eq!(account.balances().order_margin(), om);

        let limit_price = QuoteCurrency::new(120, 0);
        let order = LimitOrder::new(Sell, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(2.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut account, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(220, 0);
        assert_eq!(
            order_margin.order_margin(init_margin_req, account.position()),
            om
        );
        assert_eq!(account.balances().order_margin(), om);

        let limit_price = QuoteCurrency::new(95, 0);
        let order = LimitOrder::new(Buy, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(3.into(), 0.into());
        let order = order.into_pending(meta);
        order_margin
            .try_insert(order, &mut account, init_margin_req)
            .unwrap();
        let om = QuoteCurrency::new(220, 0);
        assert_eq!(
            order_margin.order_margin(init_margin_req, account.position()),
            om
        );
        assert_eq!(account.balances().order_margin(), om);
    }
}
