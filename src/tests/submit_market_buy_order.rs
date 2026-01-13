use std::num::NonZeroUsize;

use crate::{
    mock_exchange_linear,
    prelude::*,
    test_fee_taker,
};

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_reject() {
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

    let order = MarketOrder::new(Side::Buy, BaseCurrency::new(10, 0)).unwrap();
    assert_eq!(
        exchange.submit_market_order(order),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );
}
#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_no_position() {
    let mut exchange = mock_exchange_linear();
    let mut balances = exchange.account().balances().clone();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap()
            .is_empty()
    );

    let qty = BaseCurrency::new(5, 0);
    let order = MarketOrder::new(Side::Buy, qty).unwrap();
    exchange.submit_market_order(order).unwrap();

    // make sure its executed immediately
    let entry_price = QuoteCurrency::new(101, 0);
    let notional = QuoteCurrency::convert_from(qty, entry_price);
    let fee = notional * *test_fee_taker().as_ref();
    let init_margin = notional * init_margin_req;
    assert!(balances.try_reserve_order_margin(init_margin));
    assert_eq!(
        exchange.account().position().clone(),
        Position::Long(PositionInner::new(qty, entry_price,))
    );
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(495, 0) - fee)
            .position_margin(QuoteCurrency::new(505, 0))
            .order_margin(QuoteCurrency::new(0, 0))
            .total_fees_paid(fee)
            .build()
    );
}

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_with_long_position() {
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
    let qty = BaseCurrency::new(5, 0);
    let order = MarketOrder::new(Side::Buy, qty).unwrap();
    exchange.submit_market_order(order).unwrap();
    let entry_price = QuoteCurrency::new(100, 0);
    let fee0 = QuoteCurrency::new(3, 1);
    assert_eq!(
        exchange.account().position().clone(),
        Position::Long(PositionInner::new(qty, entry_price,))
    );
    assert!(exchange.active_limit_orders().is_empty());
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(500, 0) - fee0)
            .position_margin(QuoteCurrency::new(500, 0))
            .order_margin(QuoteCurrency::new(0, 0))
            .total_fees_paid(fee0)
            .build()
    );

    // Buy again
    let order = MarketOrder::new(Side::Buy, BaseCurrency::new(4, 0)).unwrap();
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
        Position::Long(PositionInner::new(
            BaseCurrency::new(9, 0),
            QuoteCurrency::new(100, 0),
        ))
    );
    assert!(exchange.active_limit_orders().is_empty());
}

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_with_short_position() {
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
    let order = MarketOrder::new(Side::Sell, BaseCurrency::new(9, 0)).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee0 = QuoteCurrency::new(54, 2);
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(100, 0) - fee0)
            .position_margin(QuoteCurrency::new(900, 0))
            .order_margin(QuoteCurrency::new(0, 0))
            .total_fees_paid(fee0)
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
        &ActiveLimitOrders::with_capacity(NonZeroUsize::new(10).unwrap())
    );

    // Now close the position with a buy order
    let order = MarketOrder::new(Side::Buy, BaseCurrency::new(9, 0)).unwrap();
    let fee1 = QuoteCurrency::convert_from(order.quantity(), exchange.market_state().ask())
        * *test_fee_taker().as_ref();
    exchange.submit_market_order(order).unwrap();
    assert_eq!(exchange.account().position(), &Position::Neutral);
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(1000, 0) - fee0 - fee1 - QuoteCurrency::new(9, 0))
            .position_margin(QuoteCurrency::new(0, 0))
            .order_margin(QuoteCurrency::new(0, 0))
            .total_fees_paid(fee0 + fee1)
            .build()
    );
}

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_turnaround_short() {
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

    // First enter a short position
    let qty = BaseCurrency::new(9, 0);
    let order = MarketOrder::new(Side::Sell, qty).unwrap();
    exchange.submit_market_order(order).unwrap();
    let entry_price = QuoteCurrency::new(99, 0);
    let notional = QuoteCurrency::convert_from(qty, entry_price);
    let fee_0 = notional * *test_fee_taker().as_ref();
    assert!(exchange.active_limit_orders().is_empty());
    assert_eq!(
        exchange.account().position().clone(),
        Position::Short(PositionInner::new(qty, entry_price))
    );
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(QuoteCurrency::new(109, 0) - fee_0)
            .position_margin(QuoteCurrency::new(891, 0))
            .order_margin(Zero::zero())
            .total_fees_paid(fee_0)
            .build()
    );

    // Close the entire position and buy some more
    let qty = BaseCurrency::new(18, 0);
    let order = MarketOrder::new(Side::Buy, qty).unwrap();
    exchange.submit_market_order(order).unwrap();
    let entry_price = QuoteCurrency::new(100, 0);
    let fee_1 = QuoteCurrency::convert_from(qty, entry_price) * *test_fee_taker().as_ref();
    assert!(exchange.active_limit_orders().is_empty());
    assert_eq!(
        exchange.account().position().clone(),
        Position::Long(PositionInner::new(BaseCurrency::new(9, 0), entry_price))
    );
    assert_eq!(
        exchange.account().balances(),
        &Balances::builder()
            .available(
                QuoteCurrency::new(100, 0)
                    - QuoteCurrency::new(5346, 4)
                    - QuoteCurrency::new(108, 2)
                    - QuoteCurrency::new(9, 0)
            )
            .position_margin(QuoteCurrency::new(900, 0))
            .order_margin(Zero::zero())
            .total_fees_paid(fee_0 + fee_1)
            .build()
    );
}
