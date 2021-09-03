//! Test if a pure limit order strategy works correctly

use lfest::*;

#[test]
fn limit_orders_only() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let config = Config {
        fee_maker: 0.0002,
        fee_taker: 0.0006,
        starting_balance: 1000.0,
        leverage: 1.0,
        futures_type: FuturesType::Linear,
    };
    let mut exchange = Exchange::new(config);

    let _ = exchange.update_state(100.0, 100.1, 0, 100.1, 100.0);
}
