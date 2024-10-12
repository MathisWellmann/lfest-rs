//! Test file for the linear futures mode of the exchange

use lfest::{mock_exchange_linear_with_account_tracker, prelude::*, TEST_FEE_TAKER};

#[test]
#[tracing_test::traced_test]
fn lin_long_market_win_full() {
    let starting_balance = quote!(1000);
    let mut exchange = mock_exchange_linear_with_account_tracker(starting_balance);
    let mut accounting = InMemoryTransactionAccounting::new(starting_balance);
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    let _ = exchange
        .update_state(
            0.into(),
            &Bba {
                bid: quote!(99.0),
                ask: quote!(100.0),
            },
        )
        .unwrap();
    assert_eq!(exchange.account_tracker().num_submitted_limit_orders(), 0);
    assert_eq!(exchange.account_tracker().num_cancelled_limit_orders(), 0);
    assert_eq!(
        exchange.account_tracker().num_fully_filled_limit_orders(),
        0
    );
    assert_eq!(exchange.account_tracker().num_submitted_market_orders(), 0);
    assert_eq!(exchange.account_tracker().num_filled_market_orders(), 0);
    assert_eq!(exchange.account_tracker().buy_volume(), quote!(0));
    assert_eq!(exchange.account_tracker().sell_volume(), quote!(0));
    assert_eq!(exchange.fees_paid(), quote!(0));

    let qty = base!(5);
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, qty).unwrap())
        .unwrap();
    let bid = quote!(100);
    let ask = quote!(101);
    let order_updates = exchange.update_state(0.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(exchange.account_tracker().num_submitted_limit_orders(), 0);
    assert_eq!(exchange.account_tracker().num_cancelled_limit_orders(), 0);
    assert_eq!(
        exchange.account_tracker().num_fully_filled_limit_orders(),
        0
    );
    assert_eq!(exchange.account_tracker().num_submitted_market_orders(), 1);
    assert_eq!(exchange.account_tracker().num_filled_market_orders(), 1);
    assert_eq!(exchange.account_tracker().buy_volume(), quote!(500));
    assert_eq!(exchange.account_tracker().sell_volume(), quote!(0));

    let fees = TEST_FEE_TAKER.for_value(Quote::convert_from(qty, bid));
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            qty,
            bid,
            &mut accounting,
            init_margin_req,
            fees,
        ))
    );
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), quote!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(500),
            position_margin: quote!(500),
            order_margin: quote!(0)
        }
    );

    let bid = quote!(200);
    let ask = quote!(201);
    let order_updates = exchange.update_state(0.into(), &bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());
    assert_eq!(
        exchange.position().unrealized_pnl(quote!(200), quote!(201)),
        quote!(500.0)
    );

    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, base!(5.0)).unwrap())
        .unwrap();

    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.position().unrealized_pnl(quote!(200), quote!(201)),
        quote!(0.0)
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(1499.1),
            position_margin: quote!(0),
            order_margin: quote!(0)
        }
    );
    assert_eq!(exchange.account_tracker().num_submitted_limit_orders(), 0);
    assert_eq!(exchange.account_tracker().num_cancelled_limit_orders(), 0);
    assert_eq!(
        exchange.account_tracker().num_fully_filled_limit_orders(),
        0
    );
    assert_eq!(exchange.account_tracker().num_submitted_market_orders(), 2);
    assert_eq!(exchange.account_tracker().num_filled_market_orders(), 2);
    assert_eq!(exchange.account_tracker().buy_volume(), quote!(500));
    assert_eq!(exchange.account_tracker().sell_volume(), quote!(1000));
    assert_eq!(exchange.fees_paid(), quote!(0.9));
}
