use crate::{
    mock_exchange_linear,
    prelude::*,
};

/// Reproduces an accounting hole observed in downstream backtests:
///
/// A position-reducing market order bypasses every risk check
/// (`check_market_sell_order` returns `Ok(())` early for `order.quantity() <= abs_qty`),
/// yet settling it still realizes losses and pays taker fees out of the wallet, and
/// shrinking the position strips the offset from a resting sell limit order, raising
/// its order margin requirement. Nothing re-checks or re-reserves, so the account ends
/// with `equity < position_margin + order_margin` and the next `available_balance`
/// call fails its debug assertion.
#[test]
#[should_panic(expected = "avail >= Zero::zero()")]
/// TODO: fix this panic
fn reducing_market_order_violates_balance_invariant() {
    let mut exchange = mock_exchange_linear();
    assert!(
        exchange
            .update_state(&Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 0.into()
            })
            .unwrap()
            .is_empty()
    );

    // Open a long position with almost the entire wallet:
    // 9.7 @ 101 -> position margin 979.7, taker fee 0.58782, equity 999.41218.
    exchange
        .submit_market_order(MarketOrder::new(Side::Buy, BaseCurrency::new(97, 1)).unwrap())
        .unwrap();

    // Rest a sell limit order above entry. It is almost fully offset by the long:
    // order margin = max(0, 999.1 - 979.7) = 19.4, which passes the risk check.
    exchange
        .submit_limit_order(
            LimitOrder::new(
                Side::Sell,
                QuoteCurrency::new(103, 0),
                BaseCurrency::new(97, 1),
            )
            .unwrap(),
        )
        .unwrap();

    // Partially close the long at a loss. No risk check runs for reducing orders, but:
    // - realized pnl 4 * (100 - 101) = -4 and taker fee 0.24 shrink equity to 995.17218,
    // - position margin drops to 575.7 while the resting ask is re-offset to
    //   max(0, 999.1 - 575.7) = 423.4 of order margin.
    // Total requirement 999.1 now exceeds equity: available balance is negative.
    exchange
        .submit_market_order(MarketOrder::new(Side::Sell, BaseCurrency::new(4, 0)).unwrap())
        .unwrap();

    // Observing the available balance now trips `debug_assert!(avail >= Zero::zero())`.
    let _ = exchange.account().available_balance();
}
