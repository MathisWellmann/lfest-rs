//! Test file for the linear futures mode of the exchange

use lfest::*;

#[test]
fn lin_long_market_win_full() {
  let config = Config {
      fee_maker: -0.00025,
      fee_taker: 0.00075,
      starting_balance: 1000.0,
      use_candles: false,
      leverage: 1.0,
      futures_type: FuturesType::Linear,
  };
  let fee_taker = config.fee_taker;
  let mut exchange = Exchange::new(config);
  exchange.update_state(100.0, 100.0, 0);

  assert!(exchange.submit_order(Order::market(Side::Buy, 9.0).unwrap()).is_ok());
  exchange.update_state(100.0, 100.0, 0);

  assert_eq!(exchange.account().position().size(), 9.0);
  assert_eq!(exchange.account().position().entry_price(), 100.0);
  
}

