//! Test if a pure limit order strategy works correctly

use lfest::{mock_exchange_linear, mock_exchange_linear_with_account_tracker, prelude::*};
use num_traits::Zero;

#[test]
#[tracing_test::traced_test]
fn limit_orders_only() {
    let mut exchange = mock_exchange_linear_with_account_tracker(QuoteCurrency::new(1000, 0));
    let mut balances = exchange.balances().clone();

    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let fee_maker = exchange.config().contract_spec().fee_maker();

    let bid = QuoteCurrency::new(100, 0);
    let ask = QuoteCurrency::new(101, 0);
    let exec_orders = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();
    assert_eq!(exec_orders.len(), 0);

    let qty = BaseCurrency::new(99, 1);
    let fee0 = QuoteCurrency::convert_from(qty, bid) * *fee_maker.as_ref();
    let o = LimitOrder::new(Side::Buy, bid, qty).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(
        exchange.balances(),
        &Balances {
            available: QuoteCurrency::new(10, 0),
            position_margin: QuoteCurrency::zero(),
            order_margin: QuoteCurrency::new(990, 0),
            total_fees_paid: fee0,
            _i: std::marker::PhantomData
        }
    );
    assert_eq!(exchange.balances().total_fees_paid, QuoteCurrency::zero());

    let order_updates = exchange
        .update_state(&Trade {
            price: QuoteCurrency::new(99, 0),
            quantity: BaseCurrency::new(10, 0),
            side: Side::Sell,
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let order_updates = exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(98, 0),
            ask: QuoteCurrency::new(99, 0),
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            qty,
            QuoteCurrency::new(100, 0),
            init_margin_req,
            fee0,
            &mut balances,
        ))
    );
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(exchange.market_state().bid(), exchange.market_state().ask()),
        QuoteCurrency::new(-198, 1)
    );
    assert_eq!(
        exchange.balances(),
        &Balances {
            available: QuoteCurrency::new(10, 0),
            position_margin: QuoteCurrency::new(990, 0),
            order_margin: QuoteCurrency::zero(),
            total_fees_paid: fee0,
            _i: std::marker::PhantomData
        }
    );

    let sell_price = QuoteCurrency::new(105, 0);
    let fee1 = QuoteCurrency::convert_from(qty, sell_price) * *fee_maker.as_ref();
    let o = LimitOrder::new(Side::Sell, sell_price, qty).unwrap();
    exchange.submit_limit_order(o).unwrap();

    let order_updates = exchange
        .update_state(&Trade {
            price: QuoteCurrency::new(106, 0),
            quantity: BaseCurrency::new(10, 0),
            side: Side::Buy,
            timestamp_exchange_ns: 3.into(),
        })
        .unwrap();
    assert!(!order_updates.is_empty());
    assert_eq!(
        exchange.balances(),
        &Balances {
            available: QuoteCurrency::new(10495, 1) - fee0 - fee1,
            position_margin: QuoteCurrency::zero(),
            order_margin: QuoteCurrency::zero(),
            total_fees_paid: fee0 + fee1,
            _i: std::marker::PhantomData
        }
    );
    let order_updates = exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(106, 0),
            ask: QuoteCurrency::new(107, 0),
            timestamp_exchange_ns: 4.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.balances(),
        &Balances {
            available: QuoteCurrency::new(10495, 1)
                - QuoteCurrency::new(198, 3)
                - QuoteCurrency::new(2079, 4),
            position_margin: QuoteCurrency::zero(),
            order_margin: QuoteCurrency::zero(),
            total_fees_paid: QuoteCurrency::new(4059, 4),
            _i: std::marker::PhantomData
        }
    );
    assert_eq!(
        exchange.balances().total_fees_paid,
        QuoteCurrency::new(4059, 4)
    );
}

#[test]
#[tracing_test::traced_test]
fn limit_orders_2() {
    let mut exchange = mock_exchange_linear();

    let exec_orders = exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(100, 0),
            ask: QuoteCurrency::new(101, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();
    assert!(exec_orders.is_empty());

    let o = LimitOrder::new(
        Side::Sell,
        QuoteCurrency::new(101, 0),
        BaseCurrency::new(75, 2),
    )
    .unwrap();
    exchange.submit_limit_order(o).unwrap();

    let o = LimitOrder::new(
        Side::Buy,
        QuoteCurrency::new(100, 0),
        BaseCurrency::new(5, 1),
    )
    .unwrap();
    exchange.submit_limit_order(o).unwrap();

    let exec_orders = exchange
        .update_state(&Trade {
            price: QuoteCurrency::new(98, 0),
            quantity: BaseCurrency::new(2, 0),
            side: Side::Sell,
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap()
        .clone();
    let _ = exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(98, 0),
            ask: QuoteCurrency::new(99, 0),
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert_eq!(exec_orders.len(), 1);
}
