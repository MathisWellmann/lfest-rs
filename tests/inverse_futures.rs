//! Test file for the inverse futures mode of the exchange

use lfest::{mock_exchange_inverse, prelude::*, test_fee_maker, test_fee_taker};
use num_traits::{One, Zero};

#[test]
#[tracing_test::traced_test]
fn inv_long_market_win_full() {
    let mut exchange = mock_exchange_inverse(BaseCurrency::new(1, 0));
    let mut balances = exchange.balances().clone();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let _ = exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(999, 0),
            ask: QuoteCurrency::new(1000, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();

    let value = exchange.balances().available() * BaseCurrency::new(8, 1);
    let size = QuoteCurrency::convert_from(value, exchange.market_state().ask());
    let o = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = QuoteCurrency::new(1000, 0);
    let ask = QuoteCurrency::new(1001, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    let fee_quote = size * *exchange.config().contract_spec().fee_taker().as_ref();
    let fee_1 = BaseCurrency::convert_from(fee_quote, exchange.market_state().bid());

    let fees = BaseCurrency::convert_from(size, bid) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            size,
            bid,
            init_margin_req,
            fees,
            &mut balances,
        ))
    );
    assert_eq!(
        exchange.position().unrealized_pnl(bid, ask),
        BaseCurrency::zero()
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(2, 1))
            .position_margin(BaseCurrency::new(8, 1))
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fees)
            .build()
    );

    let bid = QuoteCurrency::new(2000, 0);
    let ask = QuoteCurrency::new(2001, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(QuoteCurrency::new(2000, 0), QuoteCurrency::new(2001, 0)),
        BaseCurrency::new(4, 1)
    );

    let size = QuoteCurrency::new(800, 0);
    let fee_quote2 = size * *exchange.config().contract_spec().fee_taker().as_ref();
    let fee_base2 = BaseCurrency::convert_from(fee_quote2, QuoteCurrency::new(2000, 0));

    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(QuoteCurrency::new(2000, 0), QuoteCurrency::new(2001, 0)),
        BaseCurrency::zero()
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(14, 1) - fee_1 - fee_base2)
            .position_margin(BaseCurrency::zero())
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fees + fee_base2)
            .build()
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_long_market_loss_full() {
    let mut exchange = mock_exchange_inverse(BaseCurrency::one());
    let mut balances = exchange.balances().clone();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let bid = QuoteCurrency::new(999, 0);
    let ask = QuoteCurrency::new(1000, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    let o = MarketOrder::new(Side::Buy, QuoteCurrency::new(800, 0)).unwrap();
    exchange.submit_market_order(o).unwrap();

    let qty = QuoteCurrency::new(800, 0);
    let entry_price = QuoteCurrency::new(1000, 0);
    let fees = BaseCurrency::convert_from(qty, entry_price) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            qty,
            entry_price,
            init_margin_req,
            fees,
            &mut balances,
        ))
    );
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(QuoteCurrency::new(1000, 0), ask),
        BaseCurrency::new(0, 0)
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(2, 1))
            .position_margin(BaseCurrency::new(8, 1))
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fees)
            .build()
    );

    let bid = QuoteCurrency::new(800, 0);
    let ask = QuoteCurrency::new(801, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(QuoteCurrency::new(800, 0), QuoteCurrency::new(801, 0)),
        BaseCurrency::new(-2, 1)
    );

    let size = QuoteCurrency::new(800, 0);
    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let fee_quote0 = size * *exchange.config().contract_spec().fee_taker().as_ref();
    let fee_base0 = BaseCurrency::convert_from(fee_quote0, QuoteCurrency::new(1000, 0));

    let fee_quote1 = size * *exchange.config().contract_spec().fee_taker().as_ref();
    let fee_base1 = BaseCurrency::convert_from(fee_quote1, QuoteCurrency::new(800, 0));

    let fee_combined = fee_base0 + fee_base1;

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.position().unrealized_pnl(bid, ask),
        BaseCurrency::zero()
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(8, 1) - fee_combined)
            .position_margin(BaseCurrency::zero())
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fee_combined)
            .build()
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_short_market_win_full() {
    let mut exchange = mock_exchange_inverse(BaseCurrency::one());
    let mut balances = exchange.balances().clone();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let bid = QuoteCurrency::new(1000, 0);
    assert!(
        exchange
            .update_state(&Bba {
                bid,
                ask: QuoteCurrency::new(1001, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap()
            .is_empty()
    );

    let qty = QuoteCurrency::new(800, 0);
    let o = MarketOrder::new(Side::Sell, qty).unwrap();
    exchange.submit_market_order(o).unwrap();

    let entry_price = QuoteCurrency::new(1000, 0);
    let fees = BaseCurrency::convert_from(qty, bid) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            qty,
            entry_price,
            init_margin_req,
            fees,
            &mut balances,
        ))
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(2, 1))
            .position_margin(BaseCurrency::new(8, 1))
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fees)
            .build()
    );

    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(799, 0),
                ask: QuoteCurrency::new(800, 0),
                timestamp_exchange_ns: 1.into()
            })
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(exchange.market_state().bid(), exchange.market_state().ask()),
        BaseCurrency::new(2, 1)
    );

    let size = QuoteCurrency::new(800, 0);
    let o = MarketOrder::new(Side::Buy, size).unwrap();
    let order_err = exchange.submit_market_order(o);
    assert!(order_err.is_ok());

    let bid = QuoteCurrency::new(799, 0);
    let ask = QuoteCurrency::new(800, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    let fee_quote0 = size * *exchange.config().contract_spec().fee_taker().as_ref();
    let fee_base0 = BaseCurrency::convert_from(fee_quote0, QuoteCurrency::new(1000, 0));

    let fee_quote1 = size * *exchange.config().contract_spec().fee_taker().as_ref();
    let fee_base1 = BaseCurrency::convert_from(fee_quote1, QuoteCurrency::new(800, 0));

    let fee_combined = fee_base0 + fee_base1;

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.position().unrealized_pnl(bid, ask),
        BaseCurrency::zero()
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(12, 1) - fee_combined)
            .position_margin(BaseCurrency::zero())
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fee_combined)
            .build()
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_short_market_loss_full() {
    let mut exchange = mock_exchange_inverse(BaseCurrency::one());
    let mut balances = exchange.balances().clone();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(1000, 0),
                ask: QuoteCurrency::new(1001, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap()
            .is_empty()
    );

    let value = BaseCurrency::new(4, 1);
    let size = QuoteCurrency::convert_from(value, exchange.market_state().bid());
    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = QuoteCurrency::new(999, 0);
    let ask = QuoteCurrency::new(1000, 0);
    assert!(
        exchange
            .update_state(&Bba {
                bid,
                ask,
                timestamp_exchange_ns: 1.into()
            })
            .unwrap()
            .is_empty()
    );

    let fees = BaseCurrency::convert_from(size, ask) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            size,
            ask,
            init_margin_req,
            fees,
            &mut balances,
        ))
    );
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(QuoteCurrency::new(999, 0), QuoteCurrency::new(1000, 0)),
        BaseCurrency::zero()
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(6, 1))
            .position_margin(BaseCurrency::new(4, 1))
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fees)
            .build()
    );

    assert_eq!(
        exchange.update_state(&Bba {
            bid: QuoteCurrency::new(1999, 0),
            ask: QuoteCurrency::new(2000, 0),
            timestamp_exchange_ns: 2.into()
        }),
        Err(Error::RiskError(RiskError::Liquidate))
    );

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(79964, 5))
            .position_margin(BaseCurrency::zero())
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fees)
            .build()
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_long_market_win_partial() {
    let mut exchange = mock_exchange_inverse(BaseCurrency::one());
    let mut balances = exchange.balances().clone();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let order_updates = exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(999, 0),
            ask: QuoteCurrency::new(1000, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    let value = BaseCurrency::new(8, 1);
    let size = QuoteCurrency::convert_from(value, exchange.market_state().ask());
    let o = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = QuoteCurrency::new(1000, 0);
    let ask = QuoteCurrency::new(1001, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    let fee_0 = BaseCurrency::convert_from(size, bid) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            size,
            bid,
            init_margin_req,
            fee_0,
            &mut balances,
        ))
    );
    assert_eq!(
        exchange.position().unrealized_pnl(bid, ask),
        BaseCurrency::zero()
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(2, 1))
            .position_margin(BaseCurrency::new(8, 1))
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fee_0)
            .build()
    );

    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(2000, 0),
                ask: QuoteCurrency::new(2001, 0),
                timestamp_exchange_ns: 1.into()
            })
            .unwrap()
            .is_empty()
    );

    let size = QuoteCurrency::new(400, 0);
    let fee_1 =
        BaseCurrency::convert_from(size, QuoteCurrency::new(2000, 0)) * *test_fee_taker().as_ref();

    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(QuoteCurrency::new(2000, 0), QuoteCurrency::new(2001, 0)),
        BaseCurrency::new(4, 1)
    );

    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = QuoteCurrency::new(2000, 0);
    let ask = QuoteCurrency::new(2001, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.position().quantity(), QuoteCurrency::new(400, 0));
    assert_eq!(
        exchange.position().entry_price(),
        QuoteCurrency::new(1000, 0)
    );
    assert_eq!(exchange.position().total_cost(), BaseCurrency::new(4, 1));
    assert_eq!(
        exchange.position().unrealized_pnl(bid, ask),
        BaseCurrency::new(2, 1)
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(8, 1) - fee_0 - fee_1)
            .position_margin(BaseCurrency::new(4, 1))
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fee_0 + fee_1)
            .build()
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_long_market_loss_partial() {
    let mut exchange = mock_exchange_inverse(BaseCurrency::one());
    let mut balances = exchange.balances().clone();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let bid = QuoteCurrency::new(999, 0);
    let ask = QuoteCurrency::new(1000, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    let qty = QuoteCurrency::new(800, 0);
    let fee_0 = BaseCurrency::convert_from(qty, ask) * *test_fee_taker().as_ref();
    let o = MarketOrder::new(Side::Buy, qty).unwrap();
    exchange.submit_market_order(o).unwrap();
    assert!(
        exchange
            .update_state(&Bba {
                bid,
                ask: QuoteCurrency::new(1000, 0),
                timestamp_exchange_ns: 1.into()
            })
            .unwrap()
            .is_empty()
    );
    let entry_price = QuoteCurrency::new(1000, 0);
    let fee = BaseCurrency::convert_from(qty, entry_price) * *test_fee_taker().as_ref();
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            qty,
            entry_price,
            init_margin_req,
            fee,
            &mut balances,
        ))
    );

    let bid = QuoteCurrency::new(800, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask: QuoteCurrency::new(801, 0),
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(exchange.market_state().bid(), exchange.market_state().ask()),
        BaseCurrency::new(-2, 1)
    );

    let qty = QuoteCurrency::new(400, 0);
    let fee_1 = BaseCurrency::convert_from(qty, bid) * *test_fee_taker().as_ref();
    let o = MarketOrder::new(Side::Sell, qty).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = QuoteCurrency::new(800, 0);
    let ask = QuoteCurrency::new(801, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 3.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.position().quantity(), QuoteCurrency::new(400, 0));
    assert_eq!(
        exchange.position().entry_price(),
        QuoteCurrency::new(1000, 0)
    );
    assert_eq!(exchange.position().total_cost(), BaseCurrency::new(4, 1));
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(QuoteCurrency::new(800, 0), QuoteCurrency::new(801, 0)),
        BaseCurrency::new(-1, 1)
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(5, 1) - fee_0 - fee_1)
            .position_margin(BaseCurrency::new(4, 1))
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fee + fee_1)
            .build()
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_short_market_win_partial() {
    let mut exchange = mock_exchange_inverse(BaseCurrency::one());
    let mut balances = exchange.balances().clone();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let bid = QuoteCurrency::new(1000, 0);
    let _ = exchange
        .update_state(&Bba {
            bid,
            ask: QuoteCurrency::new(1001, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();

    let qty = QuoteCurrency::new(800, 0);
    let fee_0 = BaseCurrency::convert_from(qty, bid) * *test_fee_taker().as_ref();
    let o = MarketOrder::new(Side::Sell, qty).unwrap();
    exchange.submit_market_order(o).unwrap();
    let bid = QuoteCurrency::new(999, 0);
    let ask = QuoteCurrency::new(1000, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            qty,
            QuoteCurrency::new(1000, 0),
            init_margin_req,
            fee_0,
            &mut balances,
        ))
    );
    assert_eq!(
        exchange.position().unrealized_pnl(bid, ask),
        BaseCurrency::zero()
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(2, 1))
            .position_margin(BaseCurrency::new(8, 1))
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fee_0)
            .build()
    );

    let ask = QuoteCurrency::new(800, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(799, 0),
            ask,
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(QuoteCurrency::new(799, 0), QuoteCurrency::new(800, 0)),
        BaseCurrency::new(2, 1)
    );

    let qty = QuoteCurrency::new(400, 0);
    let fee_1 = BaseCurrency::convert_from(qty, ask) * *test_fee_taker().as_ref();
    let o = MarketOrder::new(Side::Buy, qty).unwrap();
    exchange.submit_market_order(o).unwrap();
    let bid = QuoteCurrency::new(799, 0);
    let ask = QuoteCurrency::new(800, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 3.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            QuoteCurrency::new(400, 0),
            QuoteCurrency::new(1000, 0),
            init_margin_req,
            BaseCurrency::zero(),
            &mut balances,
        ))
    );
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(QuoteCurrency::new(799, 0), QuoteCurrency::new(800, 0)),
        BaseCurrency::new(1, 1)
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(7, 1) - fee_0 - fee_1)
            .position_margin(BaseCurrency::new(4, 1))
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fee_0 + fee_1)
            .build()
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_short_market_loss_partial() {
    let mut exchange = mock_exchange_inverse(BaseCurrency::one());
    let mut balances = exchange.balances().clone();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let order_updates = exchange
        .update_state(&Bba {
            bid: QuoteCurrency::new(1000, 0),
            ask: QuoteCurrency::new(1001, 0),
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    let value = BaseCurrency::new(8, 1);
    let size = QuoteCurrency::convert_from(value, exchange.market_state().bid());
    let fee_0 = value * *test_fee_taker().as_ref();
    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = QuoteCurrency::new(999, 0);
    let ask = QuoteCurrency::new(1000, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    let fee_quote1 = size * *exchange.config().contract_spec().fee_taker().as_ref();
    let fee_base1 = BaseCurrency::convert_from(fee_quote1, ask);

    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            size,
            ask,
            init_margin_req,
            fee_base1,
            &mut balances,
        ))
    );
    assert_eq!(
        exchange.position().unrealized_pnl(bid, ask),
        BaseCurrency::zero()
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(2, 1))
            .position_margin(BaseCurrency::new(8, 1))
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fee_0 + fee_base1)
            .build()
    );

    let bid = QuoteCurrency::new(1999, 0);
    let ask = QuoteCurrency::new(2000, 0);
    assert_eq!(
        exchange.update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 2.into()
        }),
        Err(Error::RiskError(RiskError::Liquidate))
    );
    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(59928, 5))
            .position_margin(BaseCurrency::zero())
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fee_0 + fee_base1)
            .build()
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_test_market_roundtrip() {
    let mut exchange = mock_exchange_inverse(BaseCurrency::one());
    let fee_taker = exchange.config().contract_spec().fee_taker();
    let ask = QuoteCurrency::new(1000, 0);
    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(999, 0),
                ask,
                timestamp_exchange_ns: 0.into()
            })
            .unwrap()
            .is_empty()
    );

    let qty = QuoteCurrency::new(900, 0);
    let fee0 = BaseCurrency::convert_from(qty, ask) * *fee_taker.as_ref();
    let buy_order = MarketOrder::new(Side::Buy, QuoteCurrency::new(900, 0)).unwrap();
    exchange.submit_market_order(buy_order).unwrap();
    let bid = QuoteCurrency::new(1000, 0);
    let ask = QuoteCurrency::new(1001, 0);
    assert!(
        exchange
            .update_state(&Bba {
                bid,
                ask,
                timestamp_exchange_ns: 1.into()
            })
            .unwrap()
            .is_empty()
    );

    let sell_order = MarketOrder::new(Side::Sell, qty).unwrap();
    exchange.submit_market_order(sell_order).unwrap();

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.position().unrealized_pnl(bid, ask),
        BaseCurrency::zero()
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::one() - BaseCurrency::new(2, 0) * fee0)
            .position_margin(BaseCurrency::zero())
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fee0)
            .build()
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_execute_limit() {
    let mut exchange = mock_exchange_inverse(BaseCurrency::one());
    let mut balances = exchange.balances().clone();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let bid = QuoteCurrency::new(1000, 0);
    let ask = QuoteCurrency::new(1001, 0);
    let _ = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 0.into(),
        })
        .unwrap();

    let limit_price = QuoteCurrency::new(900, 0);
    let qty = QuoteCurrency::new(450, 0);
    let fee_0 = BaseCurrency::convert_from(qty, limit_price) * *test_fee_maker().as_ref();
    let o = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(5, 1))
            .position_margin(BaseCurrency::zero())
            .order_margin(BaseCurrency::new(5, 1))
            .total_fees_paid(fee_0)
            .build()
    );

    let order_updates = exchange
        .update_state(&Trade {
            price: QuoteCurrency::new(899, 0),
            quantity: QuoteCurrency::new(450, 0),
            side: Side::Sell,
            timestamp_exchange_ns: 1.into(),
        })
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let bid = QuoteCurrency::new(750, 0);
    let ask = QuoteCurrency::new(751, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.market_state().bid(), QuoteCurrency::new(750, 0));
    assert_eq!(exchange.market_state().ask(), QuoteCurrency::new(751, 0));
    assert_eq!(exchange.active_limit_orders().len(), 0);
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            qty,
            limit_price,
            init_margin_req,
            fee_0,
            &mut balances,
        ))
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(5, 1))
            .position_margin(BaseCurrency::new(5, 1))
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fee_0)
            .build()
    );

    let limit_price = QuoteCurrency::new(1000, 0);
    let fee_1 = BaseCurrency::convert_from(qty, limit_price) * *test_fee_maker().as_ref();
    let o = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    let order_updates = exchange
        .update_state(&Trade {
            price: QuoteCurrency::new(1001, 0),
            quantity: QuoteCurrency::new(450, 0),
            side: Side::Buy,
            timestamp_exchange_ns: 3.into(),
        })
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let bid = QuoteCurrency::new(1199, 0);
    let ask = QuoteCurrency::new(1200, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 4.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.active_limit_orders().len(), 0);
    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(105, 2) - fee_0 - fee_1)
            .position_margin(BaseCurrency::zero())
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fee_0 + fee_1)
            .build()
    );

    let qty = QuoteCurrency::new(600, 0);
    let limit_price = QuoteCurrency::new(1200, 0);
    let fee_2 = BaseCurrency::convert_from(qty, limit_price) * *test_fee_maker().as_ref();
    let o = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    let order_updates = exchange
        .update_state(&Trade {
            price: QuoteCurrency::new(1201, 0),
            quantity: QuoteCurrency::new(600, 0),
            side: Side::Buy,
            timestamp_exchange_ns: 5.into(),
        })
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let bid = QuoteCurrency::new(1201, 0);
    let ask = QuoteCurrency::new(1202, 0);
    let order_updates = exchange
        .update_state(&Bba {
            bid,
            ask,
            timestamp_exchange_ns: 2.into(),
        })
        .unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            qty,
            limit_price,
            init_margin_req,
            fee_2,
            &mut balances,
        ))
    );
    assert_eq!(
        exchange.balances(),
        &Balances::builder()
            .available(BaseCurrency::new(105, 2) - fee_0 - fee_1 - fee_2)
            .position_margin(BaseCurrency::new(5, 1))
            .order_margin(BaseCurrency::zero())
            .total_fees_paid(fee_0 + fee_1 + fee_2)
            .build()
    );
}
