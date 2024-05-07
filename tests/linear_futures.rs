//! Test file for the linear futures mode of the exchange

use lfest::{mock_exchange_linear, prelude::*};

#[test]
#[tracing_test::traced_test]
fn lin_long_market_win_full() {
    let mut exchange = mock_exchange_linear();
    let _ = exchange
        .update_state(
            0,
            Bba {
                bid: quote!(99.0),
                ask: quote!(100.0),
            },
        )
        .unwrap();

    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, base!(5.0)).unwrap())
        .unwrap();
    let bid = quote!(100);
    let ask = quote!(101);
    let order_updates = exchange.update_state(0, bba!(bid, ask)).unwrap();
    assert!(order_updates.is_empty());

    assert_eq!(
        exchange.position(),
        &Position::Long(PositionInner::new(base!(5.0), bid))
    );
    assert_eq!(
        exchange.position(),
        &Position::Long(PositionInner::new(base!(5.0), bid))
    );
    assert_eq!(exchange.position().unrealized_pnl(bid, ask), quote!(0.0));
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(499.7),
            position_margin: quote!(500),
            order_margin: quote!(0)
        }
    );

    let bid = quote!(200);
    let ask = quote!(201);
    let order_updates = exchange.update_state(0, bba!(bid, ask)).unwrap();
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
}
