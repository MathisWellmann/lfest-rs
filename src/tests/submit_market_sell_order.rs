use std::num::NonZeroU16;

use crate::{
    mock_exchange_linear,
    prelude::*,
    test_fee_taker,
};

#[test]
fn submit_market_sell_order_reject() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap(),
        &Vec::new()
    );

    let order = MarketOrder::new(Side::Sell, BaseCurrency::new(10, 0)).unwrap();
    assert_eq!(
        exchange.submit_market_order(order),
        Err(NotEnoughAvailableBalance.into())
    );
}

#[test]
fn submit_market_sell_order() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap(),
        &Vec::new()
    );

    let qty = BaseCurrency::new(5, 0);
    let order = MarketOrder::new(Side::Sell, qty).unwrap();

    exchange.submit_market_order(order).unwrap();
    // make sure its executed immediately
    let entry_price = QuoteCurrency::new(100, 0);
    let notional = QuoteCurrency::convert_from(qty, entry_price);
    let fees = notional * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.account().position().clone(),
        Position::Short(PositionInner::new(qty, entry_price,))
    );
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(500, 0) - fees)
            .position_margin(QuoteCurrency::new(500, 0))
            .order_margin(Zero::zero())
            .total_fees_paid(fees)
            .build()
    );
}

#[test]
fn submit_market_sell_order_with_short_position() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap(),
        &Vec::new()
    );

    // First enter a short position
    let order = MarketOrder::new(Side::Sell, BaseCurrency::new(5, 0)).unwrap();
    let fee0 = QuoteCurrency::convert_from(order.quantity(), exchange.market_state().bid())
        * *test_fee_taker().as_ref();
    exchange.submit_market_order(order).unwrap();
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(500, 0) - fee0)
            .position_margin(QuoteCurrency::new(500, 0))
            .order_margin(Zero::zero())
            .total_fees_paid(fee0)
            .build()
    );
    assert_eq!(
        exchange.account().position().clone(),
        Position::Short(PositionInner::new(
            BaseCurrency::new(5, 0),
            QuoteCurrency::new(100, 0),
        ))
    );
    assert_eq!(
        exchange.active_limit_orders(),
        &ActiveLimitOrders::with_capacity(NonZeroU16::new(10).unwrap())
    );

    // Sell again
    let order = MarketOrder::new(Side::Sell, BaseCurrency::new(4, 0)).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee1 = QuoteCurrency::new(24, 2);
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(100, 0) - fee0 - fee1)
            .position_margin(QuoteCurrency::new(900, 0))
            .order_margin(QuoteCurrency::new(0, 0))
            .total_fees_paid(fee0 + fee1)
            .build()
    );
    assert_eq!(
        exchange.account().position().clone(),
        Position::Short(PositionInner::new(
            BaseCurrency::new(9, 0),
            QuoteCurrency::new(100, 0),
        ))
    );
    assert_eq!(
        exchange.active_limit_orders(),
        &ActiveLimitOrders::with_capacity(NonZeroU16::new(10).unwrap())
    );
}

#[test]
fn submit_market_sell_order_with_long_position() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(99, 0),
                ask: QuoteCurrency::new(100, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap(),
        &Vec::new()
    );

    // First enter a long position
    let order = MarketOrder::new(Side::Buy, BaseCurrency::new(9, 0)).unwrap();
    let fee_0 = QuoteCurrency::convert_from(order.quantity(), QuoteCurrency::new(100, 0))
        * *test_fee_taker().as_ref();
    exchange.submit_market_order(order).unwrap();

    // Now close the position with a sell order
    let order = MarketOrder::new(Side::Sell, BaseCurrency::new(9, 0)).unwrap();
    let fee_1 = QuoteCurrency::convert_from(order.quantity(), QuoteCurrency::new(99, 0))
        * *test_fee_taker().as_ref();
    exchange.submit_market_order(order).unwrap();
    assert_eq!(exchange.account().position(), &Position::Neutral);
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(1000, 0) - QuoteCurrency::new(9, 0) - fee_0 - fee_1)
            .position_margin(QuoteCurrency::new(0, 0))
            .order_margin(QuoteCurrency::new(0, 0))
            .total_fees_paid(fee_0 + fee_1)
            .build()
    );
}

#[test]
fn submit_market_sell_order_turnaround_long() {
    let mut exchange = mock_exchange_linear();
    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(99, 0),
                ask: QuoteCurrency::new(100, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap()
            .is_empty()
    );

    // First enter a long position
    let qty = BaseCurrency::new(9, 0);
    let order = MarketOrder::new(Side::Buy, qty).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee0 = QuoteCurrency::convert_from(qty, exchange.market_state().ask())
        * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(100, 0) - fee0)
            .position_margin(QuoteCurrency::new(900, 0))
            .order_margin(Zero::zero())
            .total_fees_paid(fee0)
            .build()
    );
    assert_eq!(
        exchange.account().position(),
        &Position::Long(PositionInner::new(
            BaseCurrency::new(9, 0),
            QuoteCurrency::new(100, 0),
        ))
    );
    assert_eq!(
        exchange.active_limit_orders(),
        &ActiveLimitOrders::with_capacity(NonZeroU16::new(10).unwrap())
    );

    // Now reverse the position
    let qty = BaseCurrency::new(18, 0);
    let order = MarketOrder::new(Side::Sell, qty).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee1 =
        QuoteCurrency::convert_from(qty, QuoteCurrency::new(99, 0)) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(100, 0) - fee0 - fee1)
            .position_margin(QuoteCurrency::new(891, 0))
            .order_margin(QuoteCurrency::new(0, 0))
            .total_fees_paid(fee0 + fee1)
            .build()
    );
    assert_eq!(
        exchange.account().position().clone(),
        Position::Short(PositionInner::new(
            BaseCurrency::new(9, 0),
            QuoteCurrency::new(99, 0),
        ))
    );
    assert_eq!(
        exchange.active_limit_orders(),
        &ActiveLimitOrders::with_capacity(NonZeroU16::new(10).unwrap())
    );
}
