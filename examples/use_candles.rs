//! Example using aggregated candles with a simple trading logic

mod load_trades;

#[macro_use]
extern crate log;

use lfest::{Config, Exchange, Order, OrderError, Side};
use load_trades::load_trades_from_csv;
use std::time::Instant;
use trade_aggregation::{aggregate_all_trades, By, VolumeAggregator};

fn main() {
    let t0 = Instant::now(); // Used for measuring runtime

    let config = Config {
        fee_maker: -0.00025,        // Bitmex maker fee
        fee_taker: 0.001,           // Bitmex taker fee
        starting_balance_base: 1.0, // one BTC as starting wallet balance
        use_candles: true,          // make sure to set to true
        leverage: 1.0,
    };
    let mut exchange = Exchange::new(config);

    // load Bitmex:XBTUSD trades from csv file
    let trades = load_trades_from_csv("./data/Bitmex_XBTUSD_1M.csv").unwrap();

    // aggregate candles using a volume threshold of 100 BTC, therefore By::Base is used
    let mut aggregator = VolumeAggregator::new(100.0, By::Base);
    let candles = aggregate_all_trades(&trades, &mut aggregator);
    println!("aggregated all 1M trades down to {} candles", candles.len());

    for c in candles.iter() {
        let (exec_orders, liq) = exchange.consume_candle(c);
        if liq {
            println!("position liquidated");
        }
        println!("executed orders: {:?}", exec_orders);

        // Trade a fraction of the available wallet balance
        let order_size: f64 = exchange.account().margin().wallet_balance() * 0.1;

        // Some arbitrary simple strategy
        let order: Option<Order> = if c.directional_volume_ratio < 0.2 {
            Some(Order::market(Side::Buy, order_size).unwrap())
        } else if c.directional_volume_ratio > 0.8 {
            Some(Order::market(Side::Sell, order_size).unwrap())
        } else {
            None // Do nothing
        };

        match order {
            Some(order) => {
                // Submit order and handle order error here if needed
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
            None => {}
        }
    }
    println!(
        "time to simulate {} candles and {} orders: {}ms",
        candles.len(),
        exchange.account().acc_tracker().num_trades(),
        t0.elapsed().as_millis()
    );
    analyze_results(&exchange);
}

/// analyze the resulting performance metrics of the traded orders
fn analyze_results(e: &Exchange) {
    let win_ratio = e.account().acc_tracker().win_ratio();
    let profit_loss_ratio = e.account().acc_tracker().profit_loss_ratio();
    let rpnl = e.account().acc_tracker().total_rpnl();
    let sharpe = e.account().acc_tracker().sharpe();
    let sortino = e.account().acc_tracker().sortino();
    let sterling_ratio = e.account().acc_tracker().sharpe_sterling_ratio();
    let max_drawdown = e.account().acc_tracker().max_drawdown();
    let max_upnl_drawdown = e.account().acc_tracker().max_upnl_drawdown();
    let num_trades = e.account().acc_tracker().num_trades();
    let turnover = e.account().acc_tracker().turnover();
    let buy_ratio = e.account().acc_tracker().buy_ratio();
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
