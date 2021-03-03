//! Example usage of Exchange using external trade data.
//! A randomly acting agent places market buy / sell orders every 100 candles

mod load_trades;

#[macro_use]
extern crate log;

use lfest::{Config, Exchange, Order, OrderError, Side};
use load_trades::load_trades_from_csv;
use rand::{thread_rng, Rng};
use std::time::Instant;

fn main() {
    let t0 = Instant::now();

    let config = Config {
        fee_maker: -0.00025,
        fee_taker: 0.001,
        starting_balance_base: 1.0,
        use_candles: false,
        leverage: 1.0,
    };
    let mut exchange = Exchange::new(config);

    // load trades from csv file
    let trades = load_trades_from_csv("./data/Bitmex_XBTUSD_1M.csv").unwrap();

    // use random action every 100 trades to buy or sell
    let mut rng = thread_rng();

    for (i, t) in trades.iter().enumerate() {
        let liq = exchange.consume_trade(t);
        if liq {
            println!(
                "position liquidated, \
            but there could still be enough wallet_balance to open a new position"
            );
        }

        if i % 100 == 0 {
            // randomly buy or sell using a market order
            let r = rng.gen::<f64>();
            // Trade a fraction of the available wallet balance
            let order_size: f64 = exchange.margin().wallet_balance() * 0.1;
            let order: Order = if r > 0.5 {
                Order::market(Side::Sell, order_size).unwrap() // Sell using market order
            } else {
                Order::market(Side::Buy, order_size).unwrap() // Buy using market order
            };
            // Handle order error here if needed
            let response: Result<Order, OrderError> = exchange.submit_order(order);
            match response {
                Ok(order) => println!("succesfully submitted order: {:?}", order),
                Err(order_err) => match order_err {
                    OrderError::MaxActiveOrders => {
                        error!("maximum number of active orders reached")
                    }
                    OrderError::InvalidLimitPrice => error!("invalid limit price of order"),
                    OrderError::InvalidTriggerPrice => error!("invalid trigger price of order"),
                    OrderError::InvalidOrderSize => error!("invalid order size"),
                    OrderError::NotEnoughAvailableBalance => {
                        error!("not enough available balance in account")
                    }
                },
            }
        }
    }
    println!(
        "time to simulate 1 million historical trades and {} orders: {}ms",
        exchange.acc_tracker().num_trades(),
        t0.elapsed().as_millis()
    );
    analyze_results(&exchange);
}

/// analyze the resulting performance metrics of the traded orders
fn analyze_results(e: &Exchange) {
    let win_ratio = e.acc_tracker().win_ratio();
    let profit_loss_ratio = e.acc_tracker().profit_loss_ratio();
    let rpnl = e.acc_tracker().total_rpnl();
    let sharpe = e.acc_tracker().sharpe();
    let sortino = e.acc_tracker().sortino();
    let sterling_ratio = e.acc_tracker().sharpe_sterling_ratio();
    let max_drawdown = e.acc_tracker().max_drawdown();
    let max_upnl_drawdown = e.acc_tracker().max_upnl_drawdown();
    let num_trades = e.acc_tracker().num_trades();
    let turnover = e.acc_tracker().turnover();
    let buy_ratio = e.acc_tracker().buy_ratio();
    println!("win_ratio: {:.2}, profit_loss_ratio: {:.2}, rpnl: {:.2}, sharpe: {:.2}, sortino: {:.2}, sr: {:.2}, \
    dd: {:.2}, upnl_dd: {:.2}, #trades: {}, turnover: {}, buy_ratio: {:.2},",
        win_ratio,
        profit_loss_ratio,
        rpnl,
        sharpe,
        sortino,
        sterling_ratio,
        max_drawdown,
        max_upnl_drawdown,
        num_trades,
        turnover,
        buy_ratio,
    );
}
