//! Test if a pure limit order strategy works correctly

use lfest::{
    mock_exchange_linear, mock_exchange_linear_with_account_tracker, prelude::*, trade,
    MockTransactionAccounting,
};

#[test]
#[tracing_test::traced_test]
fn limit_orders_only() {
    let mut exchange = mock_exchange_linear_with_account_tracker(quote!(1000));

    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let fee_maker = exchange.config().contract_spec().fee_maker();

    let bid = quote!(100);
    let ask = quote!(101);
    let exec_orders = exchange.update_state(0.into(), bba!(bid, ask)).unwrap();
    assert_eq!(exec_orders.len(), 0);

    let qty = base!(9.9);
    let fee0 = qty.convert(bid) * fee_maker;
    let o = LimitOrder::new(Side::Buy, bid, qty).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(10),
            position_margin: quote!(0),
            order_margin: quote!(990),
        }
    );
    assert_eq!(exchange.account_tracker().num_submitted_limit_orders(), 1);
    assert_eq!(exchange.account_tracker().num_cancelled_limit_orders(), 0);
    assert_eq!(
        exchange.account_tracker().num_fully_filled_limit_orders(),
        0
    );
    assert_eq!(exchange.account_tracker().num_submitted_market_orders(), 0);
    assert_eq!(exchange.account_tracker().num_filled_market_orders(), 0);
    assert_eq!(exchange.account_tracker().buy_volume(), quote!(0));
    assert_eq!(exchange.account_tracker().sell_volume(), quote!(0));
    assert_eq!(exchange.account_tracker().cumulative_fees(), quote!(0));

    let order_updates = exchange
        .update_state(1.into(), trade!(quote!(100), base!(10), Side::Sell))
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let order_updates = exchange
        .update_state(1.into(), bba!(quote!(98), quote!(99)))
        .unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            base!(9.9),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(
        exchange
            .position()
            .unrealized_pnl(exchange.market_state().bid(), exchange.market_state().ask()),
        quote!(-19.8)
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(9.802),
            position_margin: quote!(990),
            order_margin: quote!(0)
        }
    );
    assert_eq!(exchange.account_tracker().num_submitted_limit_orders(), 1);
    assert_eq!(exchange.account_tracker().num_cancelled_limit_orders(), 0);
    assert_eq!(
        exchange.account_tracker().num_fully_filled_limit_orders(),
        1
    );
    assert_eq!(exchange.account_tracker().num_submitted_market_orders(), 0);
    assert_eq!(exchange.account_tracker().num_filled_market_orders(), 0);
    assert_eq!(exchange.account_tracker().buy_volume(), quote!(990));
    assert_eq!(exchange.account_tracker().sell_volume(), quote!(0));
    assert_eq!(exchange.account_tracker().cumulative_fees(), quote!(0.198));

    let sell_price = quote!(105);
    let fee1 = qty.convert(sell_price) * fee_maker;
    let o = LimitOrder::new(Side::Sell, sell_price, qty).unwrap();
    exchange.submit_limit_order(o).unwrap();

    let order_updates = exchange
        .update_state(2.into(), trade!(quote!(105), base!(10), Side::Buy))
        .unwrap();
    assert!(!order_updates.is_empty());
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(1049.5) - fee0 - fee1,
            position_margin: quote!(0),
            order_margin: quote!(0)
        }
    );
    let order_updates = exchange
        .update_state(2.into(), bba!(quote!(106), quote!(107)))
        .unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(1049.5) - quote!(0.198) - quote!(0.2079),
            position_margin: quote!(0),
            order_margin: quote!(0)
        }
    );
    assert_eq!(exchange.account_tracker().num_submitted_limit_orders(), 2);
    assert_eq!(exchange.account_tracker().num_cancelled_limit_orders(), 0);
    assert_eq!(
        exchange.account_tracker().num_fully_filled_limit_orders(),
        2
    );
    assert_eq!(exchange.account_tracker().num_submitted_market_orders(), 0);
    assert_eq!(exchange.account_tracker().num_filled_market_orders(), 0);
    assert_eq!(exchange.account_tracker().buy_volume(), quote!(990));
    assert_eq!(exchange.account_tracker().sell_volume(), quote!(1039.5));
    assert_eq!(exchange.account_tracker().cumulative_fees(), quote!(0.4059));
}

#[test]
#[tracing_test::traced_test]
fn limit_orders_2() {
    let mut exchange = mock_exchange_linear();

    let exec_orders = exchange
        .update_state(
            0.into(),
            Bba {
                bid: quote!(100),
                ask: quote!(101),
            },
        )
        .unwrap();
    assert!(exec_orders.is_empty());

    let o = LimitOrder::new(Side::Sell, quote!(101), base!(0.75)).unwrap();
    exchange.submit_limit_order(o).unwrap();

    let o = LimitOrder::new(Side::Buy, quote!(100), base!(0.5)).unwrap();
    exchange.submit_limit_order(o).unwrap();

    let exec_orders = exchange
        .update_state(1.into(), trade!(quote!(98), base!(2), Side::Sell))
        .unwrap();
    let _ = exchange
        .update_state(1.into(), bba!(quote!(98), quote!(99)))
        .unwrap();
    assert_eq!(exec_orders.len(), 1);
}
