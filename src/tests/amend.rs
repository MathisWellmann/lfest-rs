use std::num::{
    NonZeroU16,
    NonZeroU32,
};

use test_case::test_matrix;

use crate::{
    DECIMALS,
    EXPECT_NON_ZERO,
    mock_exchange_linear,
    prelude::*,
    utils::NoUserOrderId,
};

#[tracing_test::traced_test]
#[test_matrix([BaseCurrency::new(1, 0), BaseCurrency::new(3, 0), BaseCurrency::new(5, 0)])]
fn amend_limit_order_qty(new_qty: BaseCurrency<i64, DECIMALS>) {
    let mut exchange = mock_exchange_linear();

    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 1.into()
            })
            .unwrap()
            .is_empty()
    );
    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(100, 0),
        BaseCurrency::new(5, 0),
    )
    .unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    assert_eq!(exchange.account().active_limit_orders().num_active(), 1);

    let new_order = LimitOrder::new(Side::Buy, QuoteCurrency::new(100, 0), new_qty).unwrap();
    let existing_id: OrderId = 0.into();
    exchange
        .amend_limit_order(existing_id, new_order.clone())
        .unwrap();
    let new_id: OrderId = 1.into();
    let replaced_order = exchange
        .account()
        .active_limit_orders()
        .get_by_id(new_id, Side::Buy)
        .unwrap();
    assert_eq!(replaced_order.limit_price(), new_order.limit_price());
    assert_eq!(
        replaced_order.remaining_quantity(),
        new_order.remaining_quantity()
    );
    assert_eq!(replaced_order.side(), new_order.side());
}

#[tracing_test::traced_test]
#[test_matrix([BaseCurrency::new(1, 0), BaseCurrency::new(2, 0), BaseCurrency::new(3, 0)])]
fn amend_limit_order_qty_with_partial_fill_leading_to_cancel(new_qty: BaseCurrency<i64, DECIMALS>) {
    let mut exchange = mock_exchange_linear();

    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 1.into()
            })
            .unwrap()
            .is_empty()
    );
    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(100, 0),
        BaseCurrency::new(5, 0),
    )
    .unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    assert_eq!(exchange.account().active_limit_orders().num_active(), 1);

    exchange
        .update_state(&Trade {
            price: QuoteCurrency::new(99, 0),
            quantity: BaseCurrency::new(3, 0),
            side: Side::Sell,
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();

    let existing_id: OrderId = 0.into();
    assert_eq!(
        exchange
            .account()
            .active_limit_orders()
            .get_by_id(existing_id, Side::Buy)
            .unwrap()
            .remaining_quantity(),
        BaseCurrency::new(2, 0)
    );

    let new_order = LimitOrder::new(Side::Buy, QuoteCurrency::new(100, 0), new_qty).unwrap();
    assert_eq!(
        exchange.amend_limit_order(existing_id, new_order.clone()),
        Err(AmendLimitOrderError::AmendQtyAlreadyFilled)
    );
}

#[tracing_test::traced_test]
#[test_matrix([BaseCurrency::new(4, 0), BaseCurrency::new(5 ,0), BaseCurrency::new(6, 0)])]
fn amend_limit_order_qty_with_partial_fill(new_qty: BaseCurrency<i64, DECIMALS>) {
    let mut exchange = mock_exchange_linear();

    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 1.into()
            })
            .unwrap()
            .is_empty()
    );
    let order = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(100, 0),
        BaseCurrency::new(5, 0),
    )
    .unwrap();
    exchange.submit_limit_order(order.clone()).unwrap();
    assert_eq!(exchange.account().active_limit_orders().num_active(), 1);

    exchange
        .update_state(&Trade {
            price: QuoteCurrency::new(99, 0),
            quantity: BaseCurrency::new(3, 0),
            side: Side::Sell,
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();

    let existing_id: OrderId = 0.into();
    assert_eq!(
        exchange
            .account()
            .active_limit_orders()
            .get_by_id(existing_id, Side::Buy)
            .unwrap()
            .remaining_quantity(),
        BaseCurrency::new(2, 0)
    );

    let new_order = LimitOrder::new(Side::Buy, QuoteCurrency::new(100, 0), new_qty).unwrap();
    exchange
        .amend_limit_order(existing_id, new_order.clone())
        .unwrap();
    let new_id: OrderId = 1.into();
    let replaced_order = exchange
        .account()
        .active_limit_orders()
        .get_by_id(new_id, Side::Buy)
        .unwrap();
    assert_eq!(replaced_order.limit_price(), new_order.limit_price());
    assert_eq!(replaced_order.side(), new_order.side());
    let delta = new_qty - BaseCurrency::new(5, 0);
    assert_eq!(
        replaced_order.remaining_quantity(),
        BaseCurrency::new(2, 0) + delta
    );
}

/// Linear exchange with the mock contract at bid/ask 100/101, allowing `orders_per_second`
/// user order actions.
fn rate_limited_exchange(
    orders_per_second: u32,
) -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let config = Config::new(
        QuoteCurrency::new(1000, 0),
        NonZeroU16::new(10).expect(EXPECT_NON_ZERO),
        mock_exchange_linear().config().contract_spec().clone(),
        OrderRateLimits::new(NonZeroU32::new(orders_per_second).expect(EXPECT_NON_ZERO)),
    )
    .unwrap();
    let mut exchange = Exchange::new(config);
    exchange.update_state(&super::bba(100, 101, 1)).unwrap();
    exchange
}

fn gtc_buy(
    price: i64,
    qty: i64,
) -> LimitOrder<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId, NewOrder> {
    LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(price, 0),
        BaseCurrency::new(qty, 0),
    )
    .unwrap()
    .with_time_in_force(TimeInForce::GoodTilCancelled)
}

#[test]
fn amend_uses_a_single_rate_limit_slot() {
    let mut exchange = rate_limited_exchange(2);
    let original = exchange.submit_limit_order(gtc_buy(99, 2)).unwrap();

    // The second and last slot of this second.
    let amended = exchange
        .amend_limit_order(original.id(), gtc_buy(98, 3))
        .unwrap();
    assert_eq!(amended.limit_price(), QuoteCurrency::new(98, 0));
    assert_eq!(amended.remaining_quantity(), BaseCurrency::new(3, 0));
    assert!(!amended.is_post_only());

    assert_eq!(
        exchange.amend_limit_order(amended.id(), gtc_buy(97, 3)),
        Err(AmendLimitOrderError::RateLimitReached(
            crate::order_rate_limiter::RateLimitReached
        ))
    );
    assert_eq!(
        exchange
            .account()
            .active_limit_orders()
            .get_by_id(amended.id(), Side::Buy),
        Some(&amended)
    );
}

#[test]
fn amend_of_an_already_filled_quantity_cancels_without_another_rate_limit_slot() {
    let mut exchange = rate_limited_exchange(2);
    let original = exchange.submit_limit_order(gtc_buy(100, 5)).unwrap();
    exchange
        .update_state(&Trade {
            price: QuoteCurrency::new(99, 0),
            quantity: BaseCurrency::new(3, 0),
            side: Side::Sell,
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();

    // The amend takes the last slot, and the cancel it causes must not need another one.
    assert_eq!(
        exchange.amend_limit_order(original.id(), gtc_buy(100, 3)),
        Err(AmendLimitOrderError::AmendQtyAlreadyFilled)
    );
    assert!(exchange.account().active_limit_orders().is_empty());
    assert_eq!(
        exchange.account().position().quantity(),
        BaseCurrency::new(3, 0)
    );
}

#[test]
fn amend_distinguishes_inactive_from_unknown_order_ids() {
    let mut exchange = rate_limited_exchange(100);
    let never_issued = OrderId::from(0);
    assert_eq!(
        exchange.amend_limit_order(never_issued, gtc_buy(99, 1)),
        Err(AmendLimitOrderError::OrderIdNotFound {
            order_id: never_issued
        })
    );

    let original = exchange.submit_limit_order(gtc_buy(99, 1)).unwrap();
    assert_eq!(original.id(), never_issued);
    exchange
        .cancel_limit_order(CancelBy::OrderId(original.id()))
        .unwrap();
    assert_eq!(
        exchange.amend_limit_order(original.id(), gtc_buy(99, 1)),
        Err(AmendLimitOrderError::OrderNoLongerActive)
    );
    assert_eq!(
        exchange.amend_limit_order(OrderId::from(1), gtc_buy(99, 1)),
        Err(AmendLimitOrderError::OrderIdNotFound {
            order_id: OrderId::from(1)
        })
    );
}

/// The crossing `GoodTilCancelled` case is covered by
/// `amend_to_crossing_price_is_rejected_and_keeps_original_order`.
#[test]
fn amend_to_a_crossing_post_only_price_keeps_the_original_order() {
    let mut exchange = rate_limited_exchange(100);
    let original = exchange.submit_limit_order(gtc_buy(99, 2)).unwrap();
    let crossing_post_only = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(101, 0),
        BaseCurrency::new(2, 0),
    )
    .unwrap();
    assert!(crossing_post_only.is_post_only());
    assert!(matches!(
        exchange.amend_limit_order(original.id(), crossing_post_only),
        Err(AmendLimitOrderError::SubmitLimitOrder(
            SubmitLimitOrderError::GoodTillCrossingRejectedOrder { .. }
        ))
    ));
    assert!(exchange.account().position().quantity().is_zero());
    assert_eq!(
        exchange
            .account()
            .active_limit_orders()
            .get_by_id(original.id(), Side::Buy),
        Some(&original)
    );
}

#[test]
fn portfolio_amend_uses_a_single_rate_limit_slot() {
    let cfg = PortfolioConfig::new(
        QuoteCurrency::new(1000, 0),
        vec![MarketConfig::new(
            NonZeroU16::new(10).expect(EXPECT_NON_ZERO),
            mock_exchange_linear().config().contract_spec().clone(),
            OrderRateLimits::new(NonZeroU32::new(2).expect(EXPECT_NON_ZERO)),
        )],
    )
    .unwrap();
    let mut pf =
        PortfolioExchange::<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>::new(cfg);
    let m = MarketId::new(0);
    pf.update_market(m, &super::bba(100, 101, 1)).unwrap();
    let original = pf.submit_limit_order(m, gtc_buy(99, 2)).unwrap();

    let amended = pf
        .amend_limit_order(m, original.id(), gtc_buy(98, 3))
        .unwrap();
    assert_eq!(amended.limit_price(), QuoteCurrency::new(98, 0));
    assert!(matches!(
        pf.amend_limit_order(m, amended.id(), gtc_buy(97, 3)),
        Err(PortfolioError::AmendLimitOrder(
            AmendLimitOrderError::RateLimitReached(_)
        ))
    ));
    assert_eq!(
        pf.market(m)
            .unwrap()
            .active_limit_orders
            .get_by_id(amended.id(), Side::Buy),
        Some(&amended)
    );
}
