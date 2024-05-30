//! Test file for the inverse futures mode of the exchange

use fpdec::Decimal;
use lfest::{mock_exchange_inverse, prelude::*, trade, MockTransactionAccounting};

#[test]
#[tracing_test::traced_test]
fn inv_long_market_win_full() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let _ = exchange
        .update_state(0, bba!(quote!(999.0), quote!(1000.0)))
        .unwrap();

    let value: BaseCurrency = exchange.user_balances().available_wallet_balance * base!(0.8);
    let size = value.convert(exchange.market_state().ask());
    let o = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = quote!(1000);
    let ask = quote!(1001);
    let order_updates = exchange.update_state(1, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    let fee_quote: QuoteCurrency = size * exchange.config().contract_spec().fee_taker();
    let fee_base1: BaseCurrency = fee_quote.convert(exchange.market_state().bid());

    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            size,
            bid,
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.2) - fee_base1,
            position_margin: base!(0.8),
            order_margin: base!(0)
        }
    );

    let bid = quote!(2000);
    let ask = quote!(2001);
    let order_updates = exchange.update_state(1, bba!(bid, ask)).unwrap();
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
            available_wallet_balance: base!(1.4) - fee_base1 - fee_base2,
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
    let order_updates = exchange.update_state(0, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    let o = MarketOrder::new(Side::Buy, quote!(800.0)).unwrap();
    exchange.submit_market_order(o).unwrap();

    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            quote!(800),
            quote!(1000),
            &mut accounting,
            init_margin_req
        ))
    );
    assert_eq!(
        exchange.position().unrealized_pnl(quote!(1000), ask),
        base!(0.0)
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.19952),
            position_margin: base!(0.8),
            order_margin: base!(0)
        }
    );

    let bid = quote!(800);
    let ask = quote!(801);
    let order_updates = exchange.update_state(2, bba!(bid, ask)).unwrap();
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
    let _ = exchange
        .update_state(0, bba!(quote!(1000), quote!(1001)))
        .unwrap();

    let o = MarketOrder::new(Side::Sell, quote!(800)).unwrap();
    exchange.submit_market_order(o).unwrap();

    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            quote!(800),
            quote!(1000),
            &mut accounting,
            init_margin_req,
        ))
    );

    let _ = exchange
        .update_state(1, bba!(quote!(799), quote!(800)))
        .unwrap();
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
    let order_updates = exchange.update_state(2, bba!(bid, ask)).unwrap();
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
    let _ = exchange
        .update_state(0, bba!(quote!(1000), quote!(1001)))
        .unwrap();

    let value: BaseCurrency = BaseCurrency::new(Dec!(0.4));
    let size = value.convert(exchange.market_state().bid());
    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = quote!(999);
    let ask = quote!(1000);
    let _ = exchange.update_state(1, bba!(bid, ask)).unwrap();

    let fee_quote1 = size * exchange.config().contract_spec().fee_taker();
    let fee_base1 = fee_quote1.convert(ask);

    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            size,
            ask,
            &mut accounting,
            init_margin_req
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
            available_wallet_balance: base!(0.59976),
            position_margin: base!(0.4),
            order_margin: base!(0)
        }
    );

    let _ = exchange
        .update_state(2, bba!(quote!(1999), quote!(2000)))
        .unwrap();

    let size = quote!(400.0);
    let fee_quote2 = size * exchange.config().contract_spec().fee_taker();
    let fee_base2: BaseCurrency = fee_quote2.convert(quote!(2000.0));

    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(quote!(1999), quote!(2000)),
        base!(-0.2)
    );

    let o = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = quote!(1999);
    let ask = quote!(2000);
    let order_updates = exchange.update_state(3, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.8) - fee_base1 - fee_base2,
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
        .update_state(0, bba!(quote!(999.0), quote!(1000.0)))
        .unwrap();
    assert!(order_updates.is_empty());

    let value = BaseCurrency::new(Dec!(0.8));
    let size = value.convert(exchange.market_state().ask());
    let o = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = quote!(1000);
    let ask = quote!(1001);
    let order_updates = exchange.update_state(1, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    let fee_quote = size * exchange.config().contract_spec().fee_taker();
    let fee_base1: BaseCurrency = fee_quote.convert(exchange.market_state().bid());

    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            size,
            bid,
            &mut accounting,
            init_margin_req
        ))
    );
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.19952),
            position_margin: base!(0.8),
            order_margin: base!(0)
        }
    );

    let order_updates = exchange
        .update_state(1, bba!(quote!(2000), quote!(2001)))
        .unwrap();
    assert!(order_updates.is_empty());

    let size = quote!(400.0);
    let fee_quote2 = size * exchange.config().contract_spec().fee_taker();
    let fee_base2: BaseCurrency = fee_quote2.convert(quote!(2000.0));

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
    let order_updates = exchange.update_state(2, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            quote!(400),
            quote!(1000),
            &mut accounting,
            init_margin_req
        ))
    );
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.2));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.8) - fee_base1 - fee_base2,
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
    let order_updates = exchange.update_state(0, bba!(bid, quote!(1000.0))).unwrap();
    assert!(order_updates.is_empty());

    let o = MarketOrder::new(Side::Buy, quote!(800.0)).unwrap();
    exchange.submit_market_order(o).unwrap();
    let _ = exchange.update_state(1, bba!(bid, quote!(1000))).unwrap();

    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            quote!(800.0),
            bid,
            &mut accounting,
            init_margin_req
        ))
    );

    let order_updates = exchange
        .update_state(1, bba!(quote!(800.0), quote!(801.0)))
        .unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(exchange.market_state().bid(), exchange.market_state().ask()),
        base!(-0.2)
    );

    let o = MarketOrder::new(Side::Sell, quote!(400.0)).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = quote!(800);
    let ask = quote!(801);
    let order_updates = exchange.update_state(1, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    let fee_quote0 = quote!(800.0) * exchange.config().contract_spec().fee_taker();
    let fee_base0: BaseCurrency = fee_quote0.convert(quote!(1000.0));

    let fee_quote1 = quote!(400.0) * exchange.config().contract_spec().fee_taker();
    let fee_base1: BaseCurrency = fee_quote1.convert(quote!(800.0));

    let fee_combined: BaseCurrency = fee_base0 + fee_base1;

    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            quote!(400.0),
            bid,
            &mut accounting,
            init_margin_req
        ))
    );
    assert_eq!(
        exchange.position().unrealized_pnl(quote!(800), quote!(801)),
        base!(-0.1)
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.5) - fee_combined,
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
    let _ = exchange
        .update_state(0, bba!(quote!(1000.0), quote!(1001.0)))
        .unwrap();

    let o = MarketOrder::new(Side::Sell, quote!(800.0)).unwrap();
    exchange.submit_market_order(o).unwrap();
    let bid = quote!(999);
    let ask = quote!(1000);
    let order_updates = exchange.update_state(1, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            quote!(800),
            ask,
            &mut accounting,
            init_margin_req
        ))
    );
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.19952),
            position_margin: base!(0.8),
            order_margin: base!(0)
        }
    );

    let order_updates = exchange
        .update_state(2, bba!(quote!(799), quote!(800)))
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(
        exchange.position().unrealized_pnl(quote!(799), quote!(800)),
        base!(0.2)
    );

    let o = MarketOrder::new(Side::Buy, quote!(400.0)).unwrap();
    exchange.submit_market_order(o).unwrap();
    let bid = quote!(799);
    let ask = quote!(800);
    let order_updates = exchange.update_state(3, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    let fee_quote0 = quote!(800.0) * exchange.config().contract_spec().fee_taker();
    let fee_base0: BaseCurrency = fee_quote0.convert(quote!(1000.0));

    let fee_quote1 = quote!(400.0) * exchange.config().contract_spec().fee_taker();
    let fee_base1: BaseCurrency = fee_quote1.convert(quote!(800.0));

    let fee_combined = fee_base0 + fee_base1;

    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            quote!(400),
            quote!(800),
            &mut accounting,
            init_margin_req
        ))
    );
    assert_eq!(
        exchange.position().unrealized_pnl(quote!(799), quote!(800)),
        base!(0.1)
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.7) - fee_combined,
            position_margin: base!(0.4),
            order_margin: base!(0)
        }
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_short_market_loss_partial() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let order_updates = exchange
        .update_state(0, bba!(quote!(1000), quote!(1001)))
        .unwrap();
    assert!(order_updates.is_empty());

    let value = base!(0.8);
    let size: QuoteCurrency = value.convert(exchange.market_state().bid());
    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let bid = quote!(999);
    let ask = quote!(1000);
    let order_updates = exchange.update_state(1, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    let fee_quote1 = size * exchange.config().contract_spec().fee_taker();
    let fee_base1: BaseCurrency = fee_quote1.convert(ask);

    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            size,
            ask,
            &mut accounting,
            init_margin_req
        ))
    );
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.2) - fee_base1,
            position_margin: base!(0.8),
            order_margin: base!(0)
        }
    );

    let bid = quote!(1999);
    let ask = quote!(2000);
    let order_updates = exchange.update_state(1, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    let size = quote!(400.0);
    let fee_quote2 = size * exchange.config().contract_spec().fee_taker();
    let fee_base2: BaseCurrency = fee_quote2.convert(ask);

    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(-0.4));

    let o = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            quote!(400),
            bid,
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), base!(-0.2));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.4) - fee_base1 - fee_base2,
            position_margin: base!(0.4),
            order_margin: base!(0)
        }
    );
}

#[test]
#[tracing_test::traced_test]
fn inv_test_market_roundtrip() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let _ = exchange
        .update_state(0, bba!(quote!(999), quote!(1000)))
        .unwrap();

    let value: BaseCurrency = exchange.user_balances().available_wallet_balance * base!(0.9);
    let size = value.convert(exchange.market_state().ask());
    let buy_order = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(buy_order).unwrap();
    let _ = exchange
        .update_state(1, bba!(quote!(1000), quote!(1001)))
        .unwrap();

    let sell_order = MarketOrder::new(Side::Sell, size).unwrap();

    exchange.submit_market_order(sell_order).unwrap();

    let fee_quote = size * exchange.config().contract_spec().fee_taker();
    let fee_base: BaseCurrency = fee_quote.convert(quote!(1000.0));

    let bid = quote!(1000);
    let ask = quote!(1001);
    let order_updates = exchange.update_state(2, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(quote!(1000), quote!(1001)),
        base!(0.0)
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(1) - base!(2) * fee_base,
            position_margin: base!(0),
            order_margin: base!(0)
        }
    );

    let size = quote!(900.0);
    let buy_order = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(buy_order).unwrap();
    let _ = exchange
        .update_state(3, bba!(quote!(1000.0), quote!(1001.0)))
        .unwrap();

    let size = quote!(950.0);
    let sell_order = MarketOrder::new(Side::Sell, size).unwrap();

    exchange.submit_market_order(sell_order).unwrap();

    let bid = quote!(998);
    let ask = quote!(1000);
    let order_updates = exchange.update_state(4, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            quote!(50),
            quote!(1000),
            &mut accounting,
            init_margin_req
        ))
    );
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(quote!(998), quote!(1000)),
        base!(0.0)
    );
    // assert_eq!(exchange.account().position().margin(), base!(0.05));
    assert!(exchange.user_balances().available_wallet_balance < base!(1.0));
    todo!("compare `UserBalances`");
}

#[test]
#[tracing_test::traced_test]
fn inv_execute_limit() {
    let mut exchange = mock_exchange_inverse(base!(1));
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let bid = quote!(1000);
    let ask = quote!(1001);
    let _ = exchange.update_state(0, bba!(bid, ask)).unwrap();

    let o = LimitOrder::new(Side::Buy, quote!(900.0), quote!(450.0)).unwrap();
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
        .update_state(1, trade!(quote!(900.0), quote!(450.0), Side::Sell))
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let bid = quote!(750);
    let ask = quote!(751);
    let order_updates = exchange.update_state(1, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.market_state().bid(), quote!(750));
    assert_eq!(exchange.market_state().ask(), quote!(751));
    assert_eq!(exchange.active_limit_orders().len(), 0);
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            quote!(450),
            quote!(900),
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.4999),
            position_margin: base!(0.5),
            order_margin: base!(0)
        }
    );

    let o = LimitOrder::new(Side::Sell, quote!(1000), quote!(450)).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    let order_updates = exchange
        .update_state(1, trade!(quote!(1000), quote!(450), Side::Buy))
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let bid = quote!(1199);
    let ask = quote!(1200);
    let order_updates = exchange.update_state(1, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.active_limit_orders().len(), 0);
    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(1.04981),
            position_margin: base!(0),
            order_margin: base!(0)
        }
    );

    let o = LimitOrder::new(Side::Sell, quote!(1200), quote!(600)).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(exchange.active_limit_orders().len(), 1);

    let order_updates = exchange
        .update_state(2, trade!(quote!(1200), quote!(600), Side::Buy))
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let bid = quote!(1201);
    let ask = quote!(1202);
    let order_updates = exchange.update_state(2, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            quote!(600),
            bid,
            &mut accounting,
            init_margin_req
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: base!(0.54971),
            position_margin: base!(0.5),
            order_margin: base!(0)
        }
    );
}
