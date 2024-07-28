use hashbrown::HashMap;

use crate::{mock_exchange::MockTransactionAccounting, mock_exchange_linear, prelude::*};

#[test]
fn submit_market_sell_order_reject() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0.into(), &bba!(quote!(100), quote!(101)))
            .unwrap(),
        Vec::new()
    );

    let order = MarketOrder::new(Side::Sell, base!(10)).unwrap();
    assert_eq!(
        exchange.submit_market_order(order),
        Err(Error::RiskError(RiskError::NotEnoughAvailableBalance))
    );
}

#[test]
fn submit_market_sell_order() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert_eq!(
        exchange
            .update_state(0.into(), &bba!(quote!(100), quote!(101)))
            .unwrap(),
        Vec::new()
    );

    let order = MarketOrder::new(Side::Sell, base!(5)).unwrap();
    exchange.submit_market_order(order).unwrap();
    // make sure its excuted immediately
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            base!(5),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(500) - quote!(0.3),
            position_margin: quote!(500),
            order_margin: quote!(0)
        }
    );
}

#[test]
fn submit_market_sell_order_with_short_position() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert_eq!(
        exchange
            .update_state(0.into(), &bba!(quote!(100), quote!(101)))
            .unwrap(),
        Vec::new()
    );

    // First enter a short position
    let order = MarketOrder::new(Side::Sell, base!(5)).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee0 = quote!(0.3);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(500) - fee0,
            position_margin: quote!(500),
            order_margin: quote!(0)
        }
    );
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            base!(5),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(exchange.active_limit_orders(), &HashMap::default());

    // Sell again
    let order = MarketOrder::new(Side::Sell, base!(4)).unwrap();
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
        Position::Short(PositionInner::new(
            base!(9),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(exchange.active_limit_orders(), &HashMap::default());
}

#[test]
fn submit_market_sell_order_with_long_position() {
    let mut exchange = mock_exchange_linear();
    assert_eq!(
        exchange
            .update_state(0.into(), &bba!(quote!(99), quote!(100)))
            .unwrap(),
        Vec::new()
    );

    // First enter a long position
    let order = MarketOrder::new(Side::Buy, base!(9)).unwrap();
    exchange.submit_market_order(order).unwrap();

    // Now close the position with a sell order
    let order = MarketOrder::new(Side::Sell, base!(9)).unwrap();
    exchange.submit_market_order(order).unwrap();
    assert_eq!(exchange.position(), &Position::Neutral);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            // - fee - fee - spread loss
            available_wallet_balance: quote!(1000) - quote!(0.54) - quote!(0.5346) - quote!(9),
            position_margin: quote!(0),
            order_margin: quote!(0)
        }
    );
}

#[test]
fn submit_market_sell_order_turnaround_long() {
    let mut exchange = mock_exchange_linear();
    let mut accounting = MockTransactionAccounting::default();
    let init_margin_req = exchange.config().contract_spec().init_margin_req();
    assert!(exchange
        .update_state(0.into(), &bba!(quote!(99), quote!(100)))
        .unwrap()
        .is_empty());

    // First enter a long position
    let order = MarketOrder::new(Side::Buy, base!(9)).unwrap();
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
        Position::Long(PositionInner::new(
            base!(9),
            quote!(100),
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(exchange.active_limit_orders(), &HashMap::default());

    // Now reverse the position
    let order = MarketOrder::new(Side::Sell, base!(18)).unwrap();
    exchange.submit_market_order(order).unwrap();
    let fee1 = quote!(1.0692);
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: quote!(100) - fee0 - fee1,
            position_margin: quote!(891),
            order_margin: quote!(0)
        }
    );
    assert_eq!(
        exchange.position().clone(),
        Position::Short(PositionInner::new(
            base!(9),
            quote!(99),
            &mut accounting,
            init_margin_req,
        ))
    );
    assert_eq!(exchange.active_limit_orders(), &HashMap::default());
}
