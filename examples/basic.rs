//! Example usage of Exchange using external trade data.
//! A randomly acting agent places market buy / sell orders every 100 candles

mod load_trades;

#[macro_use]
extern crate log;

use std::time::Instant;

use lfest::*;
use load_trades::load_prices_from_csv;
use rand::{thread_rng, Rng};

fn main() {
    let t0 = Instant::now();

    let starting_wb = base!(1.0);
    let futures_type = FuturesTypes::Inverse;
    let config =
        Config::new(Fee(0.0002), Fee(0.0006), starting_wb, 1.0, futures_type, String::new(), true)
            .unwrap();

    let acc_tracker = FullAccountTracker::new(starting_wb, futures_type);
    let mut exchange = Exchange::new(acc_tracker, config);

    // load trades from csv file
    let prices = load_prices_from_csv("./data/Bitmex_XBTUSD_1M.csv").unwrap();

    // use random action every 100 trades to buy or sell
    let mut rng = thread_rng();

    for (i, p) in prices.iter().enumerate() {
        let (exec_orders, liq) = exchange.update_state(
            i as u64,
            MarketUpdate::Bba {
                bid: quote!(*p),
                ask: quote!(*p + 0.1),
            },
        );
        if liq {
            // check liquidation
        }
        if !exec_orders.is_empty() {
            println!("executed orders: {:?}", exec_orders);
        }

        if i % 100 == 0 {
            // Trade a fraction of the available wallet balance
            let order_value: BaseCurrency =
                exchange.account().margin().wallet_balance() * base!(0.1);
            let order_size = order_value.convert(exchange.bid());
            let order = if rng.gen() {
                Order::market(Side::Sell, order_size).unwrap() // Sell using
                                                               // market order
            } else {
                Order::market(Side::Buy, order_size).unwrap() // Buy using market order
            };
            // Handle order error here if needed
            match exchange.submit_order(order) {
                Ok(order) => println!("succesfully submitted order: {:?}", order),
                Err(order_err) => match order_err {
                    OrderError::MaxActiveOrders => {
                        error!("maximum number of active orders reached")
                    }
                    OrderError::InvalidLimitPrice => error!("invalid limit price of order"),
                    OrderError::InvalidTriggerPrice => error!("invalid trigger price of order"),
                    OrderError::OrderSizeMustBePositive => error!("invalid order size"),
                    OrderError::NotEnoughAvailableBalance => {
                        error!("not enough available balance in account")
                    }
                },
            }
        }
    }
    println!(
        "time to simulate 1 million historical trades and {} orders: {}ms",
        exchange.account().account_tracker().num_trades(),
        t0.elapsed().as_millis()
    );
    analyze_results(&exchange.account().account_tracker());
}

/// analyze the resulting performance metrics of the traded orders
fn analyze_results<M>(acc_tracker: &FullAccountTracker<M>)
where M: Currency + Send {
    let win_ratio = acc_tracker.win_ratio();
    let profit_loss_ratio = acc_tracker.profit_loss_ratio();
    let rpnl = acc_tracker.total_rpnl();
    let sharpe = acc_tracker.sharpe(ReturnsSource::Hourly, false);
    let sortino = acc_tracker.sortino(ReturnsSource::Hourly, false);
    let max_drawdown = acc_tracker.max_drawdown_wallet_balance();
    let max_upnl_drawdown = acc_tracker.max_drawdown_total();
    let num_trades = acc_tracker.num_trades();
    let turnover = acc_tracker.turnover();
    let buy_ratio = acc_tracker.buy_ratio();

    println!(
        "win_ratio: {:.2}, profit_loss_ratio: {:.2}, rpnl: {:.2}, sharpe: {:.2}, sortino: {:.2}, \
    dd: {:.2}, upnl_dd: {:.2}, #trades: {}, turnover: {}, buy_ratio: {:.2},",
        win_ratio,
        profit_loss_ratio,
        rpnl,
        sharpe,
        sortino,
        max_drawdown,
        max_upnl_drawdown,
        num_trades,
        turnover,
        buy_ratio,
    );
}
