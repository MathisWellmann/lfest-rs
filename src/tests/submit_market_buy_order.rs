use hashbrown::HashMap;

use crate::{
    mock_exchange::MockTransactionAccounting, mock_exchange_linear, prelude::*,
    risk_engine::RiskError,
};

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_reject() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        Vec::new()
    );

    let order = MarketOrder::new(Side::Buy, base!(10)).unwrap();
    assert_eq!(
        exchange.submit_market_order(order),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );
}
#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_no_position() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert!(exchange
        .update_state(0, bba!(quote!(100), quote!(101)))
        .unwrap()
        .is_empty());

    let order = MarketOrder::new(Side::Buy, base!(5)).unwrap();
    exchange.submit_market_order(order).unwrap();

    // make sure its excuted immediately
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            base!(5),
            quote!(101),
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(495) - quote!(0.303), // - fee ofc
            position_margin: quote!(505),
            order_margin: quote!(0)
        }
    );
}

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_with_long_position() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        Vec::new()
    );

    // First enter a long position
    let order = MarketOrder::new(Side::Buy, base!(5)).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee0 = quote!(0.3);
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            base!(5),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(exchange.active_limit_orders(), &HashMap::default());
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(500) - fee0,
            position_margin: quote!(500),
            order_margin: quote!(0)
        }
    );

    // Buy again
    let order = MarketOrder::new(Side::Buy, base!(4)).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee1 = quote!(0.24);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(100) - fee0 - fee1,
            position_margin: quote!(900),
            order_margin: quote!(0)
        }
    );
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            base!(9),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(exchange.active_limit_orders(), &HashMap::default());
}

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_with_short_position() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(100), quote!(101)))
            .unwrap(),
        Vec::new()
    );

    // First enter a short position
    let order = MarketOrder::new(Side::Sell, base!(9)).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee0 = quote!(0.54);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(100) - fee0,
            position_margin: quote!(900),
            order_margin: quote!(0)
        }
    );
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            base!(9),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(exchange.active_limit_orders(), &HashMap::default());

    // Now close the position with a buy order
    let order = MarketOrder::new(Side::Buy, base!(9)).unwrap();
    exchange.submit_market_order(order).unwrap();
    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(1000) - quote!(0.54) - quote!(0.5454) - quote!(9),
            position_margin: quote!(0),
            order_margin: quote!(0)
        }
    );
}

#[test]
#[tracing_test::traced_test]
fn submit_market_buy_order_turnaround_short() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert_eq!(
        exchange
            .update_state(0, bba!(quote!(99), quote!(100)))
            .unwrap(),
        Vec::new()
    );

    // First enter a short position
    let order = MarketOrder::new(Side::Sell, base!(9)).unwrap();
    exchange.submit_market_order(order).unwrap();
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            base!(9),
            quote!(99),
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(109) - quote!(0.5346),
            position_margin: quote!(891),
            order_margin: quote!(0)
        }
    );

    // Close the entire position and buy some more
    let order = MarketOrder::new(Side::Buy, base!(18)).unwrap();
    exchange.submit_market_order(order).unwrap();
    assert_eq!(
        exchange.position().clone(),
        Position::Long(PositionInner::new(
            base!(9),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            // - fee - fee - spread loss
            available_wallet_balance: quote!(100) - quote!(0.5346) - quote!(1.08) - quote!(9),
            position_margin: quote!(900),
            order_margin: quote!(0)
        }
    );
}
