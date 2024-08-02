//! Test file for the inverse futures mode of the exchange

use fpdec::Decimal;
use lfest::{
    mock_exchange_inverse, prelude::*, trade, MockTransactionAccounting, TEST_FEE_MAKER,
    TEST_FEE_TAKER,
};

#[test]
#[tracing_test::traced_test]
fn inv_long_market_win_full() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let _ = exchange
        .update_state(0.into(), &bba!(quote!(999.0), quote!(1000.0)))
        .unwrap();

    let value: BaseCurrency = exchange.user_balances().available_wallet_balance * base!(0.8);
    let size = value.convert(exchange.market_state().ask());
    let o = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = quote!(1000);
    let ask = quote!(1001);
    let order_updates = exchange.update_state(1.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    let fee_quote: QuoteCurrency = size * exchange.config().contract_spec().fee_taker();
    let fee_1: BaseCurrency = fee_quote.convert(exchange.market_state().bid());

    let fees = size.convert(bid) * TEST_FEE_TAKER;
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            size,
            bid,
            &mut accounting,
            init_margin_req,
            fees,
        ))
    );
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.2),
            position_margin: base!(0.8),
            order_margin: base!(0)
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), fee_1);

    let bid = quote!(2000);
    let ask = quote!(2001);
    let order_updates = exchange.update_state(1.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(quote!(2000), quote!(2001)),
        base!(0.4)
    );

    let size = quote!(800.0);
    let fee_quote2: QuoteCurrency = size * exchange.config().contract_spec().fee_taker();
    let fee_base2: BaseCurrency = fee_quote2.convert(quote!(2000.0));

    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(quote!(2000), quote!(2001)),
        base!(0.0)
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(1.4) - fee_1 - fee_base2,
            position_margin: base!(0),
            order_margin: base!(0)
        }
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_long_market_loss_full() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let bid = quote!(999);
    let ask = quote!(1000);
    let order_updates = exchange.update_state(0.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    let o = MarketOrder::new(Side::Buy, quote!(800.0)).unwrap();
    exchange.submit_market_order(o).unwrap();

    let qty = quote!(800);
    let entry_price = quote!(1000);
    let fees = qty.convert(entry_price) * TEST_FEE_TAKER;
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fees,
        ))
    );
    assert_eq!(
        exchange.position().unrealized_pnl(quote!(1000), ask),
        base!(0.0)
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.2),
            position_margin: base!(0.8),
            order_margin: base!(0)
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), fees);

    let bid = quote!(800);
    let ask = quote!(801);
    let order_updates = exchange.update_state(2.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange.position().unrealized_pnl(quote!(800), quote!(801)),
        base!(-0.2)
    );

    let size = quote!(800.0);
    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let fee_quote0 = size * exchange.config().contract_spec().fee_taker();
    let fee_base0 = fee_quote0.convert(quote!(1000.0));

    let fee_quote1 = size * exchange.config().contract_spec().fee_taker();
    let fee_base1 = fee_quote1.convert(quote!(800.0));

    let fee_combined = fee_base0 + fee_base1;

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.8) - fee_combined,
            position_margin: base!(0),
            order_margin: base!(0)
        }
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_short_market_win_full() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let fee_taker = exchange.config().contract_spec().fee_taker();
    let bid = quote!(1000);
    assert!(exchange
        .update_state(0.into(), &bba!(bid, quote!(1001)))
        .unwrap()
        .is_empty());

    let qty = quote!(800);
    let fee0 = qty.convert(bid) * fee_taker;
    let o = MarketOrder::new(Side::Sell, qty).unwrap();
    exchange.submit_market_order(o).unwrap();

    let entry_price = quote!(1000);
    let fees = qty.convert(entry_price) * TEST_FEE_TAKER;
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fees,
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.2),
            position_margin: base!(0.8),
            order_margin: base!(0)
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), fee0);

    assert!(exchange
        .update_state(1.into(), &bba!(quote!(799), quote!(800)))
        .unwrap()
        .is_empty());
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(exchange.market_state().bid(), exchange.market_state().ask()),
        base!(0.2)
    );

    let size = quote!(800);
    let o = MarketOrder::new(Side::Buy, size).unwrap();
    let order_err = exchange.submit_market_order(o);
    assert!(order_err.is_ok());

    let bid = quote!(799);
    let ask = quote!(800);
    let order_updates = exchange.update_state(2.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    let fee_quote0 = size * exchange.config().contract_spec().fee_taker();
    let fee_base0 = fee_quote0.convert(quote!(1000));

    let fee_quote1 = size * exchange.config().contract_spec().fee_taker();
    let fee_base1 = fee_quote1.convert(quote!(800));

    let fee_combined: BaseCurrency = fee_base0 + fee_base1;

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(1.2) - fee_combined,
            position_margin: base!(0),
            order_margin: base!(0)
        }
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_short_market_loss_full() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert!(exchange
        .update_state(0.into(), &bba!(quote!(1000), quote!(1001)))
        .unwrap()
        .is_empty());

    let value: BaseCurrency = BaseCurrency::new(Dec!(0.4));
    let size = value.convert(exchange.market_state().bid());
    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = quote!(999);
    let ask = quote!(1000);
    assert!(exchange
        .update_state(1.into(), &bba!(bid, ask))
        .unwrap()
        .is_empty());

    let fees = size.convert(ask) * TEST_FEE_TAKER;
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            size,
            ask,
            &mut accounting,
            init_margin_req,
            fees,
        ))
    );
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(quote!(999), quote!(1000)),
        base!(0.0)
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.6),
            position_margin: base!(0.4),
            order_margin: base!(0)
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), fees);

    assert_eq!(
        exchange.update_state(2.into(), &bba!(quote!(1999), quote!(2000))),
        Err(Error::RiskError(RiskError::Liquidate))
    );

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.79964),
            position_margin: base!(0),
            order_margin: base!(0)
        }
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_long_market_win_partial() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let order_updates = exchange
        .update_state(0.into(), &bba!(quote!(999.0), quote!(1000.0)))
        .unwrap();
    assert!(order_updates.is_empty());

    let value = BaseCurrency::new(Dec!(0.8));
    let size = value.convert(exchange.market_state().ask());
    let o = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = quote!(1000);
    let ask = quote!(1001);
    let order_updates = exchange.update_state(1.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    let fee_0 = size.convert(bid) * TEST_FEE_TAKER;
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            size,
            bid,
            &mut accounting,
            init_margin_req,
            fee_0
        ))
    );
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.2),
            position_margin: base!(0.8),
            order_margin: base!(0)
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), fee_0);

    assert!(exchange
        .update_state(1.into(), &bba!(quote!(2000), quote!(2001)))
        .unwrap()
        .is_empty());

    let size = quote!(400.0);
    let fee_1 = size.convert(quote!(2000)) * TEST_FEE_TAKER;

    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(quote!(2000), quote!(2001)),
        base!(0.4)
    );

    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = quote!(2000);
    let ask = quote!(2001);
    let order_updates = exchange.update_state(2.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.position().quantity(), quote!(400));
    assert_eq!(exchange.position().entry_price(), quote!(1000));
    assert_eq!(exchange.position().total_cost(), base!(0.4));
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.2));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.8) - fee_0 - fee_1,
            position_margin: base!(0.4),
            order_margin: base!(0)
        }
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_long_market_loss_partial() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let bid = quote!(999);
    let ask = quote!(1000);
    let order_updates = exchange.update_state(0.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    let qty = quote!(800);
    let fee_0 = qty.convert(ask) * TEST_FEE_TAKER;
    let o = MarketOrder::new(Side::Buy, qty).unwrap();
    exchange.submit_market_order(o).unwrap();
    assert!(exchange
        .update_state(1.into(), &bba!(bid, quote!(1000)))
        .unwrap()
        .is_empty());
    let entry_price = quote!(1000);
    let fee = qty.convert(entry_price) * TEST_FEE_TAKER;
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            qty,
            entry_price,
            &mut accounting,
            init_margin_req,
            fee,
        ))
    );

    let bid = quote!(800);
    let order_updates = exchange
        .update_state(1.into(), &bba!(bid, quote!(801.0)))
        .unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(exchange.market_state().bid(), exchange.market_state().ask()),
        base!(-0.2)
    );

    let qty = quote!(400);
    let fee_1 = qty.convert(bid) * TEST_FEE_TAKER;
    let o = MarketOrder::new(Side::Sell, qty).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = quote!(800);
    let ask = quote!(801);
    let order_updates = exchange.update_state(1.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.position().quantity(), quote!(400));
    assert_eq!(exchange.position().entry_price(), quote!(1000));
    assert_eq!(exchange.position().total_cost(), base!(0.4));
    assert_eq!(exchange.position().outstanding_fees(), base!(0));
    assert_eq!(
        exchange.position().unrealized_pnl(quote!(800), quote!(801)),
        base!(-0.1)
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.5) - fee_0 - fee_1,
            position_margin: base!(0.4),
            order_margin: base!(0)
        }
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_short_market_win_partial() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let bid = quote!(1000);
    let _ = exchange
        .update_state(0.into(), &bba!(bid, quote!(1001.0)))
        .unwrap();

    let qty = quote!(800);
    let fee_0 = qty.convert(bid) * TEST_FEE_TAKER;
    let o = MarketOrder::new(Side::Sell, qty).unwrap();
    exchange.submit_market_order(o).unwrap();
    let bid = quote!(999);
    let ask = quote!(1000);
    let order_updates = exchange.update_state(1.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            qty,
            quote!(1000),
            &mut accounting,
            init_margin_req,
            fee_0,
        ))
    );
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.2),
            position_margin: base!(0.8),
            order_margin: base!(0)
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), fee_0);

    let ask = quote!(800);
    let order_updates = exchange
        .update_state(2.into(), &bba!(quote!(799), ask))
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(
        exchange.position().unrealized_pnl(quote!(799), quote!(800)),
        base!(0.2)
    );

    let qty = quote!(400);
    let fee_1 = qty.convert(ask) * TEST_FEE_TAKER;
    let o = MarketOrder::new(Side::Buy, qty).unwrap();
    exchange.submit_market_order(o).unwrap();
    let bid = quote!(799);
    let ask = quote!(800);
    let order_updates = exchange.update_state(3.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            quote!(400),
            quote!(1000),
            &mut accounting,
            init_margin_req,
            base!(0),
        ))
    );
    assert_eq!(
        exchange.position().unrealized_pnl(quote!(799), quote!(800)),
        base!(0.1)
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.7) - fee_0 - fee_1,
            position_margin: base!(0.4),
            order_margin: base!(0)
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), base!(0));
}

#[test]
#[tracing_test::traced_test]
fn inv_short_market_loss_partial() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let order_updates = exchange
        .update_state(0.into(), &bba!(quote!(1000), quote!(1001)))
        .unwrap();
    assert!(order_updates.is_empty());

    let value = base!(0.8);
    let size: QuoteCurrency = value.convert(exchange.market_state().bid());
    let fee_0 = value * TEST_FEE_TAKER;
    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = quote!(999);
    let ask = quote!(1000);
    let order_updates = exchange.update_state(1.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    let fee_quote1 = size * exchange.config().contract_spec().fee_taker();
    let fee_base1: BaseCurrency = fee_quote1.convert(ask);

    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            size,
            ask,
            &mut accounting,
            init_margin_req,
            fee_base1,
        ))
    );
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.2),
            position_margin: base!(0.8),
            order_margin: base!(0)
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), fee_0);

    let bid = quote!(1999);
    let ask = quote!(2000);
    assert_eq!(
        exchange.update_state(1.into(), &bba!(bid, ask)),
        Err(Error::RiskError(RiskError::Liquidate))
    );
    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.59928),
            position_margin: base!(0),
            order_margin: base!(0)
        }
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_test_market_roundtrip() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let fee_taker = exchange.config().contract_spec().fee_taker();
    let ask = quote!(1000);
    assert!(exchange
        .update_state(0.into(), &bba!(quote!(999), ask))
        .unwrap()
        .is_empty());

    let qty = quote!(900);
    let fee0 = qty.convert(ask) * fee_taker;
    let buy_order = MarketOrder::new(Side::Buy, quote!(900)).unwrap();
    exchange.submit_market_order(buy_order).unwrap();
    let bid = quote!(1000);
    let ask = quote!(1001);
    assert!(exchange
        .update_state(1.into(), &bba!(bid, ask))
        .unwrap()
        .is_empty());

    let sell_order = MarketOrder::new(Side::Sell, qty).unwrap();
    exchange.submit_market_order(sell_order).unwrap();

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(1) - base!(2) * fee0,
            position_margin: base!(0),
            order_margin: base!(0)
        }
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_execute_limit() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let bid = quote!(1000);
    let ask = quote!(1001);
    let _ = exchange.update_state(0.into(), &bba!(bid, ask)).unwrap();

    let limit_price = quote!(900);
    let qty = quote!(450);
    let o = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.5),
            position_margin: base!(0),
            order_margin: base!(0.5)
        }
    );

    let order_updates = exchange
        .update_state(1.into(), &trade!(quote!(899), quote!(450), Side::Sell))
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let bid = quote!(750);
    let ask = quote!(751);
    let order_updates = exchange.update_state(1.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.market_state().bid(), quote!(750));
    assert_eq!(exchange.market_state().ask(), quote!(751));
    assert_eq!(exchange.active_limit_orders().len(), 0);
    let fee_0 = qty.convert(limit_price) * TEST_FEE_MAKER;
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            qty,
            limit_price,
            &mut accounting,
            init_margin_req,
            fee_0,
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.5),
            position_margin: base!(0.5),
            order_margin: base!(0)
        }
    );
    assert_eq!(exchange.position().outstanding_fees(), fee_0);

    let limit_price = quote!(1000);
    let fee_1 = qty.convert(limit_price) * TEST_FEE_MAKER;
    let o = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    let order_updates = exchange
        .update_state(1.into(), &trade!(quote!(1001), quote!(450), Side::Buy))
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let bid = quote!(1199);
    let ask = quote!(1200);
    let order_updates = exchange.update_state(1.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.active_limit_orders().len(), 0);
    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(1.05) - fee_0 - fee_1,
            position_margin: base!(0),
            order_margin: base!(0)
        }
    );

    let qty = quote!(600);
    let limit_price = quote!(1200);
    let fee_2 = qty.convert(limit_price) * TEST_FEE_MAKER;
    let o = LimitOrder::new(Side::Sell, limit_price, qty).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    let order_updates = exchange
        .update_state(2.into(), &trade!(quote!(1201), quote!(600), Side::Buy))
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let bid = quote!(1201);
    let ask = quote!(1202);
    let order_updates = exchange.update_state(2.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            qty,
            limit_price,
            &mut accounting,
            init_margin_req,
            fee_2
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(1.05) - fee_0 - fee_1 - qty.convert(limit_price),
            position_margin: base!(0.5),
            order_margin: base!(0)
        }
    );
}
