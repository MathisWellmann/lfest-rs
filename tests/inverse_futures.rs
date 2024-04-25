//! Test file for the inverse futures mode of the exchange

use fpdec::Decimal;
use lfest::{mock_exchange_quote, prelude::*, trade};

#[test]
fn inv_long_market_win_full() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let mut exchange = mock_exchange_quote(base!(1));
    let _ = exchange
        .update_state(0, bba!(quote!(999.0), quote!(1000.0)))
        .unwrap();

    let value: BaseCurrency = exchange.account().available_balance() * base!(0.8);
    let size = value.convert(exchange.market_state().ask());
    let o = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let _ = exchange
        .update_state(1, bba!(quote!(1000), quote!(1001)))
        .unwrap();

    let fee_quote = size.fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base1 = fee_quote.convert(exchange.market_state().bid());

    assert_eq!(exchange.account().position().size(), size);
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(1000), quote!(1001)),
        base!(0.0)
    );
    assert_eq!(exchange.account().wallet_balance(), base!(1.0) - fee_base1);
    assert_eq!(exchange.account().position().position_margin(), base!(0.8));
    assert_eq!(
        exchange.account().available_balance(),
        (base!(0.2) - fee_base1)
    );

    let _ = exchange
        .update_state(1, bba!(quote!(2000), quote!(2001)))
        .unwrap();
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(2000), quote!(2001)),
        base!(0.4)
    );

    let size = quote!(800.0);
    let fee_base2 = size.fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_asset2 = fee_base2.convert(quote!(2000.0));

    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    assert_eq!(exchange.account().position().size(), quote!(0.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(2000), quote!(2001)),
        base!(0.0)
    );
    assert_eq!(
        exchange.account().wallet_balance(),
        (base!(1.4) - fee_base1 - fee_asset2)
    );
    assert_eq!(exchange.account().position().position_margin(), base!(0.0));
    assert_eq!(
        exchange.account().available_balance(),
        (base!(1.4) - fee_base1 - fee_asset2)
    );
}

#[test]
fn inv_long_market_loss_full() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let mut exchange = mock_exchange_quote(base!(1));
    let _ = exchange
        .update_state(0, bba!(quote!(999), quote!(1000)))
        .unwrap();

    let o = MarketOrder::new(Side::Buy, quote!(800.0)).unwrap();
    exchange.submit_market_order(o).unwrap();

    assert_eq!(exchange.account().position().size(), quote!(800.0));
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(1000), quote!(1001)),
        base!(0.0)
    );
    assert_eq!(exchange.account().wallet_balance(), base!(0.99952));
    assert_eq!(exchange.account().available_balance(), base!(0.19952));
    assert_eq!(exchange.account().order_margin(), base!(0.0));
    assert_eq!(exchange.account().position().position_margin(), base!(0.8));

    let _ = exchange
        .update_state(2, bba!(quote!(800), quote!(801)))
        .unwrap();
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(800), quote!(801)),
        base!(-0.2)
    );

    let size = quote!(800.0);
    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let fee_quote0 = size.fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base0 = fee_quote0.convert(quote!(1000.0));

    let fee_quote1 = size.fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base1 = fee_quote1.convert(quote!(800.0));

    let fee_combined = fee_base0 + fee_base1;

    assert_eq!(exchange.account().position().size(), quote!(0.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(800), quote!(801)),
        base!(0.0)
    );
    assert_eq!(
        exchange.account().wallet_balance(),
        (base!(0.8) - fee_combined)
    );
    assert_eq!(exchange.account().position().position_margin(), base!(0.0));
    assert_eq!(
        exchange.account().available_balance(),
        (base!(0.8) - fee_combined)
    );
}

#[test]
fn inv_short_market_win_full() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let mut exchange = mock_exchange_quote(base!(1));
    let _ = exchange
        .update_state(0, bba!(quote!(1000), quote!(1001)))
        .unwrap();

    let o = MarketOrder::new(Side::Sell, quote!(800)).unwrap();
    exchange.submit_market_order(o).unwrap();

    assert_eq!(exchange.account().position().size(), quote!(-800));

    let _ = exchange
        .update_state(1, bba!(quote!(799), quote!(800)))
        .unwrap();
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(exchange.market_state().bid(), exchange.market_state().ask()),
        base!(0.2)
    );

    let size = quote!(800);
    let o = MarketOrder::new(Side::Buy, size).unwrap();
    let order_err = exchange.submit_market_order(o);
    assert!(order_err.is_ok());
    let _ = exchange
        .update_state(2, bba!(quote!(799), quote!(800)))
        .unwrap();

    let fee_quote0 = size.fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base0 = fee_quote0.convert(quote!(1000));

    let fee_quote1 = size.fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base1 = fee_quote1.convert(quote!(800));

    let fee_combined: BaseCurrency = fee_base0 + fee_base1;

    assert_eq!(exchange.account().position().size(), quote!(0.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(799), quote!(800)),
        base!(0.0)
    );
    assert_eq!(
        exchange.account().wallet_balance(),
        base!(1.2) - fee_combined
    );
    assert_eq!(exchange.account().position().position_margin(), base!(0.0));
    assert_eq!(
        exchange.account().available_balance(),
        base!(1.2) - fee_combined
    );
}

#[test]
fn inv_short_market_loss_full() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let mut exchange = mock_exchange_quote(base!(1));
    let _ = exchange
        .update_state(0, bba!(quote!(1000), quote!(1001)))
        .unwrap();

    let value: BaseCurrency = BaseCurrency::new(Dec!(0.4));
    let size = value.convert(exchange.market_state().bid());
    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let _ = exchange
        .update_state(1, bba!(quote!(999), quote!(1000)))
        .unwrap();

    let fee_quote1 = size.fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base1 = fee_quote1.convert(quote!(1000));

    assert_eq!(exchange.account().position().size(), size.into_negative());
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(999), quote!(1000)),
        base!(0.0)
    );
    assert_eq!(exchange.account().wallet_balance(), base!(1.0) - fee_base1);
    assert_eq!(
        exchange.account().position().position_margin().inner(),
        Dec!(0.4)
    );
    assert_eq!(
        exchange.account().available_balance().inner(),
        Dec!(0.59976)
    );

    let _ = exchange
        .update_state(2, bba!(quote!(1999), quote!(2000)))
        .unwrap();

    let size = quote!(400.0);
    let fee_quote2 = size.fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base2: BaseCurrency = fee_quote2.convert(quote!(2000.0));

    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(1999), quote!(2000)),
        base!(-0.2)
    );

    let o = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let _ = exchange
        .update_state(3, bba!(quote!(1999), quote!(2000)))
        .unwrap();

    assert_eq!(exchange.account().position().size(), quote!(0.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(1999), quote!(2000)),
        base!(0.0)
    );
    assert_eq!(
        exchange.account().wallet_balance(),
        (base!(0.8) - fee_base1 - fee_base2)
    );
    assert_eq!(exchange.account().position().position_margin(), base!(0.0));
    assert_eq!(
        exchange.account().available_balance(),
        (base!(0.8) - fee_base1 - fee_base2)
    );
}

#[test]
fn inv_long_market_win_partial() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let mut exchange = mock_exchange_quote(base!(1));
    let _ = exchange
        .update_state(0, bba!(quote!(999.0), quote!(1000.0)))
        .unwrap();

    let value = BaseCurrency::new(Dec!(0.8));
    let size = value.convert(exchange.market_state().ask());
    let o = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let _ = exchange
        .update_state(1, bba!(quote!(1000), quote!(1001)))
        .unwrap();

    let fee_quote = size.fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base1: BaseCurrency = fee_quote.convert(exchange.market_state().bid());

    assert_eq!(exchange.account().position().size(), size);
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(1000), quote!(1001)),
        base!(0.0)
    );
    assert_eq!(exchange.account().wallet_balance(), base!(1.0) - fee_base1);
    assert_eq!(
        exchange.account().position().position_margin().inner(),
        Dec!(0.8)
    );
    assert_eq!(
        exchange.account().available_balance().inner(),
        Dec!(0.19952)
    );

    let _ = exchange
        .update_state(1, bba!(quote!(2000), quote!(2001)))
        .unwrap();

    let size = quote!(400.0);
    let fee_quote2 = size.fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base2: BaseCurrency = fee_quote2.convert(quote!(2000.0));

    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(2000), quote!(2001))
            .inner(),
        Dec!(0.4)
    );

    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let _ = exchange
        .update_state(2, bba!(quote!(2000), quote!(2001)))
        .unwrap();

    assert_eq!(exchange.account().position().size().inner(), Dec!(400.0));
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(2000), quote!(2001))
            .inner(),
        Dec!(0.2)
    );
    assert_eq!(
        exchange.account().wallet_balance(),
        base!(1.2) - fee_base1 - fee_base2
    );
    assert_eq!(
        exchange.account().position().position_margin().inner(),
        Dec!(0.4)
    );
    assert_eq!(
        exchange.account().available_balance().inner(),
        (base!(0.8) - fee_base1 - fee_base2).inner()
    );
}

#[test]
fn inv_long_market_loss_partial() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let mut exchange = mock_exchange_quote(base!(1));
    let _ = exchange
        .update_state(0, bba!(quote!(999.0), quote!(1000.0)))
        .unwrap();

    let o = MarketOrder::new(Side::Buy, quote!(800.0)).unwrap();
    exchange.submit_market_order(o).unwrap();
    let _ = exchange
        .update_state(1, bba!(quote!(999), quote!(1000)))
        .unwrap();

    assert_eq!(exchange.account().position().size(), quote!(800.0));

    let _ = exchange
        .update_state(1, bba!(quote!(800.0), quote!(801.0)))
        .unwrap();
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(exchange.market_state().bid(), exchange.market_state().ask()),
        base!(-0.2)
    );

    let o = MarketOrder::new(Side::Sell, quote!(400.0)).unwrap();
    exchange.submit_market_order(o).unwrap();
    let _ = exchange
        .update_state(1, bba!(quote!(800), quote!(801)))
        .unwrap();

    let fee_quote0 =
        quote!(800.0).fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base0: BaseCurrency = fee_quote0.convert(quote!(1000.0));

    let fee_quote1 =
        quote!(400.0).fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base1: BaseCurrency = fee_quote1.convert(quote!(800.0));

    let fee_combined: BaseCurrency = fee_base0 + fee_base1;

    assert_eq!(exchange.account().position().size(), quote!(400.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(800), quote!(801)),
        base!(-0.1)
    );
    assert_eq!(
        exchange.account().wallet_balance(),
        (base!(0.9) - fee_combined)
    );
    assert_eq!(exchange.account().position().position_margin(), base!(0.4));
    assert_eq!(
        exchange.account().available_balance(),
        (base!(0.5) - fee_combined)
    );
}

#[test]
fn inv_short_market_win_partial() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let mut exchange = mock_exchange_quote(base!(1));
    let _ = exchange
        .update_state(0, bba!(quote!(1000.0), quote!(1001.0)))
        .unwrap();

    let o = MarketOrder::new(Side::Sell, quote!(800.0)).unwrap();
    exchange.submit_market_order(o).unwrap();
    let _ = exchange
        .update_state(1, bba!(quote!(999), quote!(1000)))
        .unwrap();

    assert_eq!(exchange.account().position().size(), quote!(-800.0));
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(999), quote!(1000)),
        base!(0.0)
    );
    assert_eq!(exchange.account().wallet_balance(), base!(0.99952));
    assert_eq!(exchange.account().available_balance(), base!(0.19952));
    assert_eq!(exchange.account().order_margin(), base!(0.0));
    assert_eq!(exchange.account().position().position_margin(), base!(0.8));

    let _ = exchange
        .update_state(2, bba!(quote!(799), quote!(800)))
        .unwrap();

    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(799), quote!(800)),
        base!(0.2)
    );

    let o = MarketOrder::new(Side::Buy, quote!(400.0)).unwrap();
    exchange.submit_market_order(o).unwrap();
    let _ = exchange
        .update_state(3, bba!(quote!(799), quote!(800)))
        .unwrap();

    let fee_quote0 =
        quote!(800.0).fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base0: BaseCurrency = fee_quote0.convert(quote!(1000.0));

    let fee_quote1 =
        quote!(400.0).fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base1: BaseCurrency = fee_quote1.convert(quote!(800.0));

    let fee_combined = fee_base0 + fee_base1;

    assert_eq!(exchange.account().position().size(), quote!(-400.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(799), quote!(800)),
        base!(0.1)
    );
    assert_eq!(
        exchange.account().wallet_balance(),
        (base!(1.1) - fee_combined)
    );
    assert_eq!(exchange.account().position().position_margin(), base!(0.4));
    assert_eq!(
        exchange.account().available_balance(),
        (base!(0.7) - fee_combined)
    );
}

#[test]
fn inv_short_market_loss_partial() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let mut exchange = mock_exchange_quote(base!(1));
    let _ = exchange
        .update_state(0, bba!(quote!(1000), quote!(1001)))
        .unwrap();

    let value = base!(0.8);
    let size: QuoteCurrency = value.convert(exchange.market_state().bid());
    let o = MarketOrder::new(Side::Sell, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    let _ = exchange
        .update_state(1, bba!(quote!(999), quote!(1000)))
        .unwrap();

    let fee_quote1 = size.fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base1: BaseCurrency = fee_quote1.convert(quote!(1000));

    assert_eq!(exchange.account().position().size(), size.into_negative());
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(999), quote!(1000)),
        base!(0.0)
    );
    assert_eq!(exchange.account().wallet_balance(), base!(1.0) - fee_base1);
    assert_eq!(exchange.account().position().position_margin(), base!(0.8));
    assert_eq!(
        exchange.account().available_balance(),
        (base!(0.2) - fee_base1)
    );

    let _ = exchange
        .update_state(1, bba!(quote!(1999), quote!(2000)))
        .unwrap();

    let size = quote!(400.0);
    let fee_quote2 = size.fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base2: BaseCurrency = fee_quote2.convert(quote!(2000.0));

    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(1999), quote!(2000)),
        base!(-0.4)
    );

    let o = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(o).unwrap();

    assert_eq!(exchange.account().position().size(), quote!(-400.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(1999), quote!(2000)),
        base!(-0.2)
    );
    assert_eq!(
        exchange.account().wallet_balance(),
        (base!(0.8) - fee_base1 - fee_base2)
    );
    assert_eq!(exchange.account().position().position_margin(), base!(0.4));
    assert_eq!(
        exchange.account().available_balance(),
        (base!(0.4) - fee_base1 - fee_base2)
    );
}

#[test]
fn inv_test_market_roundtrip() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let mut exchange = mock_exchange_quote(base!(1));
    let _ = exchange
        .update_state(0, bba!(quote!(999), quote!(1000)))
        .unwrap();

    let value: BaseCurrency = exchange.account().available_balance() * base!(0.9);
    let size = value.convert(exchange.market_state().ask());
    let buy_order = MarketOrder::new(Side::Buy, size).unwrap();
    exchange.submit_market_order(buy_order).unwrap();
    let _ = exchange
        .update_state(1, bba!(quote!(1000), quote!(1001)))
        .unwrap();

    let sell_order = MarketOrder::new(Side::Sell, size).unwrap();

    exchange.submit_market_order(sell_order).unwrap();

    let fee_quote = size.fee_portion(exchange.config().contract_specification().fee_taker);
    let fee_base: BaseCurrency = fee_quote.convert(quote!(1000.0));

    let _ = exchange
        .update_state(2, bba!(quote!(1000.0), quote!(1001.0)))
        .unwrap();

    assert_eq!(exchange.account().position().size(), quote!(0.0));
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(1000), quote!(1001)),
        base!(0.0)
    );
    assert_eq!(
        exchange.account().wallet_balance(),
        base!(1.0) - base!(2.0) * fee_base
    );
    assert_eq!(exchange.account().position().position_margin(), base!(0.0));
    assert_eq!(
        exchange.account().available_balance(),
        base!(1.0) - base!(2.0) * fee_base
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

    let _ = exchange
        .update_state(4, bba!(quote!(998.0), quote!(1000.0)))
        .unwrap();

    assert_eq!(exchange.account().position().size(), quote!(-50.0));
    assert_eq!(exchange.account().position().entry_price(), quote!(1000.0));
    assert_eq!(
        exchange
            .account()
            .position()
            .unrealized_pnl(quote!(998), quote!(1000)),
        base!(0.0)
    );
    assert!(exchange.account().wallet_balance() < base!(1.0));
    assert_eq!(exchange.account().position().position_margin(), base!(0.05));
    assert!(exchange.account().available_balance() < base!(1.0));
}

#[test]
fn inv_execute_limit() {
    if let Err(_) = pretty_env_logger::try_init() {}

    let mut exchange = mock_exchange_quote(base!(1));
    let _ = exchange
        .update_state(0, bba!(quote!(1000.0), quote!(1001.0)))
        .unwrap();

    let o = LimitOrder::new(Side::Buy, quote!(900.0), quote!(450.0)).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(exchange.account().active_limit_orders().len(), 1);
    assert_eq!(exchange.account().wallet_balance(), base!(1.0));
    assert_eq!(exchange.account().available_balance(), base!(0.49990));
    assert_eq!(exchange.account().position().position_margin(), base!(0.0));
    assert_eq!(exchange.account().order_margin(), base!(0.5001)); // this includes the fee too

    let order_updates = exchange
        .update_state(1, trade!(quote!(900.0), quote!(450.0), Side::Sell))
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let _ = exchange
        .update_state(1, bba!(quote!(750.0), quote!(751.0)))
        .unwrap();

    assert_eq!(exchange.market_state().bid(), quote!(750));
    assert_eq!(exchange.market_state().ask(), quote!(751));
    assert_eq!(exchange.account().active_limit_orders().len(), 0);
    assert_eq!(exchange.account().position().size(), quote!(450));
    assert_eq!(exchange.account().position().entry_price(), quote!(900));
    assert_eq!(exchange.account().wallet_balance(), base!(0.9999));
    assert_eq!(exchange.account().available_balance(), base!(0.4999));
    assert_eq!(exchange.account().position().position_margin(), base!(0.5));
    assert_eq!(exchange.account().order_margin(), base!(0));

    let o = LimitOrder::new(Side::Sell, quote!(1000), quote!(450)).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(exchange.account().active_limit_orders().len(), 1);

    let order_updates = exchange
        .update_state(1, trade!(quote!(1000), quote!(450), Side::Buy))
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let _ = exchange
        .update_state(1, bba!(quote!(1199), quote!(1200)))
        .unwrap();

    assert_eq!(exchange.account().active_limit_orders().len(), 0);
    assert_eq!(exchange.account().position().size(), quote!(0));
    assert_eq!(exchange.account().position().position_margin(), base!(0));
    assert_eq!(exchange.account().wallet_balance(), base!(1.04981));
    assert_eq!(exchange.account().available_balance(), base!(1.04981));

    let o = LimitOrder::new(Side::Sell, quote!(1200), quote!(600)).unwrap();
    exchange.submit_limit_order(o).unwrap();
    assert_eq!(exchange.account().active_limit_orders().len(), 1);

    let order_updates = exchange
        .update_state(2, trade!(quote!(1200), quote!(600), Side::Buy))
        .unwrap();
    assert_eq!(order_updates.len(), 1);
    let _ = exchange
        .update_state(2, bba!(quote!(1201), quote!(1202)))
        .unwrap();
    assert_eq!(exchange.account().position().size(), quote!(-600.0));
    assert_eq!(exchange.account().position().position_margin(), base!(0.5));
    assert_eq!(exchange.account().wallet_balance(), base!(1.04971));
    assert_eq!(exchange.account().available_balance(), base!(0.54971));
}
