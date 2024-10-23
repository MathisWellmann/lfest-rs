use crate::{bba, exchange::CancelBy, mock_exchange_linear, prelude::*};

#[test]
fn cancel_limit_order() {
    let mut exchange = mock_exchange_linear();
    exchange
        .update_state(
            0.into(),
            &bba!(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0)),
        )
        .unwrap();

    let limit_price = QuoteCurrency::new(100, 0);
    let qty = BaseCurrency::one();
    let order = LimitOrder::new(Side::Buy, limit_price, qty).unwrap();

    exchange.submit_limit_order(order.clone()).unwrap();

    let order_id: OrderId = 0.into();
    let meta = ExchangeOrderMeta::new(order_id, 0.into());
    let expected_order = order.into_pending(meta);

    assert_eq!(exchange.active_limit_orders().len(), 1);
    assert_eq!(
        exchange.active_limit_orders().get_by_id(order_id).unwrap(),
        &expected_order
    );
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(900, 0),
            position_margin: QuoteCurrency::zero(),
            order_margin: QuoteCurrency::new(100, 0)
        }
    );
    assert_eq!(
        exchange.position().outstanding_fees(),
        QuoteCurrency::zero()
    );

    exchange
        .cancel_limit_order(CancelBy::OrderId(order_id))
        .unwrap();
    assert!(exchange.active_limit_orders().is_empty());
    assert_eq!(
        exchange.user_balances(),
        UserBalances {
            available_wallet_balance: QuoteCurrency::new(1000, 0),
            position_margin: QuoteCurrency::zero(),
            order_margin: QuoteCurrency::zero()
        }
    );
}
