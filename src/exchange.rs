extern crate trade_aggregation;

use crate::acc_tracker::AccTracker;
use crate::{
    max, min, Account, Config, FeeType, Margin, Order, OrderError, OrderType, Position, Side,
    Validator,
};
use trade_aggregation::*;

const MAX_NUM_LIMIT_ORDERS: usize = 50;
const MAX_NUM_STOP_ORDERS: usize = 50;

#[derive(Debug, Clone)]
/// The main leveraged futures exchange for simulated trading
pub struct Exchange {
    config: Config,
    account: Account,
    validator: Validator,
    bid: f64,
    ask: f64,
    init: bool,
    next_order_id: u64,
    step: u64, // used for synhcronizing orders
    high: f64,
    low: f64,
}

impl Exchange {
    /// Create a new Exchange with the desired config and whether to use candles as infomation source
    pub fn new(config: Config) -> Exchange {
        assert!(config.leverage > 0.0);
        let account = Account::new(config.leverage, config.starting_balance_base);
        let validator = Validator::new(config.fee_maker, config.fee_taker);
        Exchange {
            config,
            account,
            validator,
            bid: 0.0,
            ask: 0.0,
            init: true,
            next_order_id: 0,
            step: 0,
            high: 0.0,
            low: 0.0,
        }
    }

    /// Return the bid price
    pub fn bid(&self) -> f64 {
        self.bid
    }

    /// Return the ask price
    pub fn ask(&self) -> f64 {
        self.ask
    }

    /// Return a reference to Account
    pub fn account(&self) -> &Account {
        &self.account
    }

    /// Return a mutable reference to Account
    pub fn account_mut(&mut self) -> &mut Account {
        &mut self.account
    }

    /// Update the exchange state with a new trade.
    /// ### Returns
    /// executed orders
    /// true if position has been liquidated
    pub fn consume_trade(&mut self, trade: &Trade) -> (Vec<Order>, bool) {
        assert!(!self.config.use_candles);

        if self.init {
            self.init = false;
            self.bid = trade.price;
            self.ask = trade.price;
        }
        if trade.size > 0.0 {
            self.ask = trade.price;
        } else {
            self.bid = trade.price;
        }

        self.validator.update(trade.price, trade.price);

        if self.check_liquidation() {
            self.liquidate();
            return (vec![], true);
        }

        self.check_orders();

        self.account.update(trade.price, trade.timestamp as u64);

        self.step += 1;

        return (self.account.executed_orders(), false);
    }

    /// Update the exchange status with a new candle.
    /// ### Returns
    /// executed orders
    /// true if position has been liquidated
    pub fn consume_candle(&mut self, candle: &Candle) -> (Vec<Order>, bool) {
        assert!(self.config.use_candles);

        self.bid = candle.close;
        self.ask = candle.close;
        self.high = candle.high;
        self.low = candle.low;

        self.validator.update(candle.close, candle.close);

        if self.check_liquidation() {
            self.liquidate();
            return (vec![], true);
        }

        self.check_orders();

        self.account.update(candle.close, candle.timestamp as u64);

        self.step += 1;

        return (self.account.executed_orders(), false);
    }

    /// Check if a liquidation event should occur
    fn check_liquidation(&mut self) -> bool {
        // TODO: check_liquidation
        // TODO: test check_liquidation

        false
    }

    /// Execute a market order
    fn execute_market(&mut self, side: Side, amount_quote: f64) {
        let price: f64 = match side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        };

        let fee_quote = self.config.fee_taker * amount_quote;
        let fee_base = fee_quote / price;
        self.account.deduce_fees(fee_base);
        self.account.change_position(side, amount_quote, price);
    }

    /// Execute a limit order, once triggered
    fn execute_limit(&mut self, side: Side, price: f64, amount_quote: f64) {
        // TODO: log_limit_order_fill
        //self.account.acc_tracker_mut().log_limit_order_fill();

        let fee_quote = self.config.fee_maker * amount_quote;
        let fee_base = fee_quote / price;
        self.account.deduce_fees(fee_base);
        self.account.change_position(side, amount_quote, price);
    }

    /// Perform a liquidation of the account
    fn liquidate(&mut self) {
        // TODO: better liquidate
        debug!("liquidating");
        if self.account.position().size() > 0.0 {
            self.execute_market(Side::Sell, self.account.position().size());
        } else {
            self.execute_market(Side::Buy, self.account.position().size().abs());
        }
    }

    /// Check if any active orders have been triggered by the most recent price action
    /// method is called after new external data has been consumed
    fn check_orders(&mut self) {
        for i in 0..self.account.active_limit_orders().len() {
            match self.account.active_limit_orders()[i].order_type {
                OrderType::Limit => self.handle_limit_order(i),
                _ => panic!("there should only be limit orders in active_limit_orders"),
            }
        }
        for i in 0..self.account.active_stop_orders().len() {
            match self.account.active_stop_orders()[i].order_type {
                OrderType::StopMarket => self.handle_stop_market_order(i),
                _ => panic!("there should only be stop market orders in active_stop_orders"),
            }
        }
    }

    /// Handle stop market order trigger and execution
    fn handle_stop_market_order(&mut self, order_idx: usize) {
        // check if stop order has been triggered
        match self.account().active_stop_orders()[order_idx].side {
            Side::Buy => match self.config.use_candles {
                true => {
                    if self.account().active_stop_orders()[order_idx].trigger_price > self.high {
                        return;
                    }
                }
                false => {
                    if self.account().active_stop_orders()[order_idx].trigger_price > self.ask {
                        return;
                    }
                }
            },
            Side::Sell => match self.config.use_candles {
                true => {
                    if self.account().active_stop_orders()[order_idx].trigger_price < self.low {
                        return;
                    }
                }
                false => {
                    if self.account().active_stop_orders()[order_idx].trigger_price > self.bid {
                        return;
                    }
                }
            },
        }
        self.execute_market(
            self.account().active_stop_orders()[order_idx].side,
            self.account().active_stop_orders()[order_idx].size,
        );
        self.account.finalize_stop_order(order_idx);
    }

    /// Handle limit order trigger and execution
    fn handle_limit_order(&mut self, order_idx: usize) {
        let o: &Order = &self.account.active_limit_orders()[order_idx];
        match o.side {
            Side::Buy => {
                match self.config.use_candles {
                    true => {
                        // use candle information to specify execution
                        if self.low <= o.limit_price {
                            self.execute_limit(o.side, o.limit_price, o.size);
                        } else {
                            return;
                        }
                    }
                    false => {
                        // use trade information ( bid / ask ) to specify if order executed
                        if self.bid < o.limit_price {
                            // execute
                            self.execute_limit(o.side, o.limit_price, o.size);
                        } else {
                            return;
                        }
                    }
                }
            }
            Side::Sell => {
                match self.config.use_candles {
                    true => {
                        // use candle information to specify execution
                        if self.high >= o.limit_price {
                            self.execute_limit(o.side, o.limit_price, o.size);
                        } else {
                            return;
                        }
                    }
                    false => {
                        // use trade information ( bid / ask ) to specify if order executed
                        if self.ask > o.limit_price {
                            // execute
                            self.execute_limit(o.side, o.limit_price, o.size);
                        } else {
                            return;
                        }
                    }
                }
            }
        }
        self.account.finalize_limit_order(order_idx);
    }

    /// Submit a new order to the exchange.
    /// Returns the order with timestamp and id filled in or OrderError
    pub fn submit_order(&mut self, mut order: Order) -> Result<Order, OrderError> {
        match order.order_type {
            OrderType::StopMarket => {
                if self.account().active_limit_orders().len() >= MAX_NUM_LIMIT_ORDERS {
                    return Err(OrderError::MaxActiveOrders);
                }
            }
            _ => {
                if self.account().active_stop_orders().len() >= MAX_NUM_STOP_ORDERS {
                    return Err(OrderError::MaxActiveOrders);
                }
            }
        }

        self.validator.validate(&order, &self.account)?;

        // assign unique order id
        order.id = self.next_order_id;
        self.next_order_id += 1;

        order.timestamp = self.step;

        return match order.order_type {
            OrderType::Market => {
                // immediately execute market order
                self.execute_market(order.side, order.size);

                Ok(order)
            }
            _ => {
                self.account.append_order(order.clone());

                Ok(order)
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::round;

    #[test]
    fn submit_order_limit() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        // submit working market order
        let o = Order::market(Side::Buy, 500.0).unwrap();
        exchange.submit_order(o).unwrap();

        let o = Order::limit(Side::Buy, 900.0, 250.0).unwrap();
        exchange.submit_order(o).unwrap();
        assert_eq!(exchange.account().active_limit_orders().len(), 1);

        // submit opposite limit order acting as target order
        let o = Order::limit(Side::Sell, 1200.0, 500.0).unwrap();
        exchange.submit_order(o).unwrap();
        assert_eq!(exchange.account().active_limit_orders().len(), 2);
    }

    #[test]
    fn test_handle_limit_order() {
        // TODO:
    }

    #[test]
    fn handle_stop_market_order_w_trade() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::stop_market(Side::Buy, 1010.0, 100.0).unwrap();
        exchange.submit_order(o).unwrap();
        assert_eq!(exchange.account().active_stop_orders().len(), 1);

        let t = Trade {
            timestamp: 2,
            price: 1010.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.account().position().size(), 100.0);
        assert_eq!(exchange.account().position().entry_price(), 1010.0);
    }

    #[test]
    fn handle_stop_market_order_w_candle() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: true,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config);

        let c = Candle {
            timestamp: 0,
            open: 40_000.0,
            high: 40_500.0,
            low: 35_000.0,
            close: 40_100.0,
            volume: 0.0,
            directional_trade_ratio: 0.0,
            directional_volume_ratio: 0.0,
            num_trades: 0,
            arithmetic_mean_price: 0.0,
            weighted_price: 0.0,
            std_dev_prices: 0.0,
            std_dev_sizes: 0.0,
            time_velocity: 0.0,
        };
        exchange.consume_candle(&c);

        let o = Order::stop_market(Side::Buy, 40_600.0, 4060.0).unwrap();
        exchange.submit_order(o).unwrap();

        let c = Candle {
            timestamp: 0,
            open: 40_100.0,
            high: 40_700.0,
            low: 36_000.0,
            close: 40_500.0,
            volume: 0.0,
            directional_trade_ratio: 0.0,
            directional_volume_ratio: 0.0,
            num_trades: 0,
            arithmetic_mean_price: 0.0,
            weighted_price: 0.0,
            std_dev_prices: 0.0,
            std_dev_sizes: 0.0,
            time_velocity: 0.0,
        };
        exchange.consume_candle(&c);

        assert_eq!(exchange.account().position().size(), 4060.0);
        assert_eq!(round(exchange.account().position().value(), 1), 0.1);
        assert_eq!(round(exchange.account().margin().position_margin(), 1), 0.1);
    }

    #[test]
    fn long_market_win_full() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.account().margin().available_balance() * 0.8;
        let size = exchange.ask * value;
        let o = Order::market(Side::Buy, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        let fee_base = size * fee_taker;
        let fee_asset1 = fee_base / exchange.bid;

        assert_eq!(exchange.account().position().size(), size);
        assert_eq!(exchange.account().position().entry_price(), 1000.0);
        assert_eq!(exchange.account().position().value(), value);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
        assert_eq!(
            exchange.account().margin().wallet_balance(),
            1.0 - fee_asset1
        );
        assert_eq!(exchange.account().margin().position_margin(), 0.8);
        assert_eq!(
            round(exchange.account().margin().available_balance(), 4),
            round(0.2 - fee_asset1, 4)
        );

        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.4);

        let size = 800.0;
        let fee_base2 = size * fee_taker;
        let fee_asset2 = fee_base2 / 2000.0;

        let o = Order::market(Side::Sell, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        assert_eq!(exchange.account().position().size(), 0.0);
        assert_eq!(exchange.account().position().value(), 0.0);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
        assert_eq!(
            exchange.account().margin().wallet_balance(),
            1.4 - fee_asset1 - fee_asset2
        );
        assert_eq!(exchange.account().margin().position_margin(), 0.0);
        assert_eq!(
            exchange.account().margin().available_balance(),
            1.4 - fee_asset1 - fee_asset2
        );
    }

    #[test]
    fn long_market_loss_full() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let config = Config {
            fee_maker: 0.0,
            fee_taker: 0.0,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 800.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.account().position().size(), 800.0);
        assert_eq!(exchange.account().position().value(), 0.8);
        assert_eq!(exchange.account().position().entry_price(), 1000.0);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
        assert_eq!(exchange.account().margin().wallet_balance(), 1.0);
        assert_eq!(
            round(exchange.account().margin().available_balance(), 2),
            0.2
        );
        assert_eq!(exchange.account().margin().order_margin(), 0.0);
        assert_eq!(exchange.account().margin().position_margin(), 0.8);

        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.account.position().unrealized_pnl(), -0.2);

        let o = Order::market(Side::Sell, 800.0).unwrap();
        exchange.submit_order(o).unwrap();
        exchange.check_orders();

        let fee_base0 = fee_taker * 800.0;
        let fee_asset0 = fee_base0 / 1000.0;

        let fee_base1 = fee_taker * 800.0;
        let fee_asset1 = fee_base1 / 800.0;

        let fee_combined = fee_asset0 + fee_asset1;

        assert_eq!(exchange.account().position().size(), 0.0);
        assert_eq!(exchange.account().position().value(), 0.0);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
        assert_eq!(
            round(exchange.account().margin().wallet_balance(), 5),
            round(0.8 - fee_combined, 5)
        );
        assert_eq!(exchange.account().margin().position_margin(), 0.0);
        assert_eq!(
            round(exchange.account().margin().available_balance(), 5),
            round(0.8 - fee_combined, 5)
        );
    }

    #[test]
    fn short_market_win_full() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Sell, 800.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.account().position().size(), -800.0);

        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.account.position().unrealized_pnl(), 0.2);

        let o = Order::market(Side::Buy, 800.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let fee_base0 = fee_taker * 800.0;
        let fee_asset0 = fee_base0 / 1000.0;

        let fee_base1 = fee_taker * 800.0;
        let fee_asset1 = fee_base1 / 800.0;

        let fee_combined = fee_asset0 + fee_asset1;

        assert_eq!(exchange.account().position().size(), 0.0);
        assert_eq!(exchange.account().position().value(), 0.0);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
        assert_eq!(
            exchange.account().margin().wallet_balance(),
            1.2 - fee_combined
        );
        assert_eq!(exchange.account().margin().position_margin(), 0.0);
        assert_eq!(
            exchange.account().margin().available_balance(),
            1.2 - fee_combined
        );
    }

    #[test]
    fn short_market_loss_full() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.account.margin().available_balance() * 0.4;
        let size = exchange.ask * value;
        let o = Order::market(Side::Sell, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        let fee_base1 = size * fee_taker;
        let fee_asset1 = fee_base1 / exchange.bid;

        assert_eq!(exchange.account().position().size(), -size);
        assert_eq!(exchange.account().position().entry_price(), 1000.0);
        assert_eq!(exchange.account().position().value(), value);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
        assert_eq!(
            exchange.account().margin().wallet_balance(),
            1.0 - fee_asset1
        );
        assert_eq!(exchange.account().margin().position_margin(), 0.4);
        assert_eq!(
            exchange.account().margin().available_balance(),
            0.6 - fee_asset1
        );

        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let size = 400.0;
        let fee_base2 = size * fee_taker;
        let fee_asset2 = fee_base2 / 2000.0;

        assert_eq!(exchange.account.position().unrealized_pnl(), -0.2);

        let o = Order::market(Side::Buy, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.account().position().size(), 0.0);
        assert_eq!(exchange.account().position().value(), 0.0);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
        assert_eq!(
            round(exchange.account().margin().wallet_balance(), 5),
            round(0.8 - fee_asset1 - fee_asset2, 5)
        );
        assert_eq!(exchange.account().margin().position_margin(), 0.0);
        assert_eq!(
            round(exchange.account().margin().available_balance(), 5),
            round(0.8 - fee_asset1 - fee_asset2, 5)
        );
    }

    #[test]
    fn long_market_win_partial() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.account.margin().available_balance() * 0.8;
        let size = exchange.ask * value;
        let o = Order::market(Side::Buy, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        let fee_base = size * fee_taker;
        let fee_asset1 = fee_base / exchange.bid;

        assert_eq!(exchange.account().position().size(), size);
        assert_eq!(exchange.account().position().entry_price(), 1000.0);
        assert_eq!(exchange.account().position().value(), value);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
        assert_eq!(
            exchange.account().margin().wallet_balance(),
            1.0 - fee_asset1
        );
        assert_eq!(exchange.account().margin().position_margin(), 0.8);
        assert_eq!(
            round(exchange.account().margin().available_balance(), 4),
            round(0.2 - fee_asset1, 4)
        );

        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let size = 400.0;
        let fee_base2 = size * fee_taker;
        let fee_asset2 = fee_base2 / 2000.0;

        assert_eq!(exchange.account().position().unrealized_pnl(), 0.4);

        let o = Order::market(Side::Sell, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.account().position().size(), 400.0);
        assert_eq!(exchange.account().position().value(), 0.2);
        assert_eq!(exchange.account().position().entry_price(), 1000.0);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.2);
        assert_eq!(
            exchange.account().margin().wallet_balance(),
            1.2 - fee_asset1 - fee_asset2
        );
        assert_eq!(exchange.account().margin().position_margin(), 0.4);
        assert_eq!(
            round(exchange.account().margin().available_balance(), 5),
            round(0.8 - fee_asset1 - fee_asset2, 5)
        );
    }

    #[test]
    fn long_market_loss_partial() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Buy, 800.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.account().position().size(), 800.0);

        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.account.position().unrealized_pnl(), -0.2);

        let o = Order::market(Side::Sell, 400.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let fee_base0 = fee_taker * 800.0;
        let fee_asset0 = fee_base0 / 1000.0;

        let fee_base1 = fee_taker * 400.0;
        let fee_asset1 = fee_base1 / 800.0;

        let fee_combined = fee_asset0 + fee_asset1;

        assert_eq!(exchange.account().position().size(), 400.0);
        assert_eq!(exchange.account().position().value(), 0.5);
        assert_eq!(exchange.account().position().unrealized_pnl(), -0.1);
        assert_eq!(
            round(exchange.account().margin().wallet_balance(), 6),
            round(0.9 - fee_combined, 6)
        );
        assert_eq!(exchange.account().margin().position_margin(), 0.4);
        assert_eq!(
            round(exchange.account().margin().available_balance(), 6),
            round(0.5 - fee_combined, 6)
        );
    }

    #[test]
    fn short_market_win_partial() {
        let config = Config {
            fee_maker: 0.0,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let o = Order::market(Side::Sell, 800.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        assert_eq!(exchange.account().position().size(), -800.0);
        assert_eq!(exchange.account().position().value(), 0.8);
        assert_eq!(exchange.account().position().entry_price(), 1000.0);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
        assert_eq!(exchange.account().margin().wallet_balance(), 0.9994);
        assert_eq!(
            round(exchange.account().margin().available_balance(), 3),
            0.199
        );
        assert_eq!(exchange.account().margin().order_margin(), 0.0);
        assert_eq!(exchange.account().margin().position_margin(), 0.8);

        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 800.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.account.position().unrealized_pnl(), 0.2);

        let o = Order::market(Side::Buy, 400.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let fee_base0 = fee_taker * 800.0;
        let fee_asset0 = fee_base0 / 1000.0;

        let fee_base1 = fee_taker * 400.0;
        let fee_asset1 = fee_base1 / 800.0;

        let fee_combined = fee_asset0 + fee_asset1;

        assert_eq!(exchange.account().position().size(), -400.0);
        assert_eq!(exchange.account().position().value(), 0.5);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.1);
        assert_eq!(
            round(exchange.account().margin().wallet_balance(), 6),
            round(1.1 - fee_combined, 6)
        );
        assert_eq!(exchange.account().margin().position_margin(), 0.4);
        assert_eq!(
            round(exchange.account().margin().available_balance(), 6),
            round(0.7 - fee_combined, 6)
        );
    }

    #[test]
    fn short_market_loss_partial() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.account.margin().available_balance() * 0.8;
        let size = exchange.ask * value;
        let o = Order::market(Side::Sell, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        exchange.check_orders();

        let fee_base1 = size * fee_taker;
        let fee_asset1 = fee_base1 / exchange.bid;

        assert_eq!(exchange.account().position().size(), -size);
        assert_eq!(exchange.account().position().entry_price(), 1000.0);
        assert_eq!(exchange.account().position().value(), value);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
        assert_eq!(
            exchange.account().margin().wallet_balance(),
            1.0 - fee_asset1
        );
        assert_eq!(exchange.account().margin().position_margin(), 0.8);
        assert_eq!(
            round(exchange.account().margin().available_balance(), 4),
            round(0.2 - fee_asset1, 4)
        );

        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let size = 400.0;
        let fee_base2 = size * fee_taker;
        let fee_asset2 = fee_base2 / 2000.0;

        assert_eq!(exchange.account().position().unrealized_pnl(), -0.4);

        let o = Order::market(Side::Buy, size).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());

        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 2000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.account().position().size(), -400.0);
        assert_eq!(exchange.account().position().value(), 0.2);
        assert_eq!(exchange.account().position().unrealized_pnl(), -0.2);
        assert_eq!(
            round(exchange.account().margin().wallet_balance(), 5),
            round(0.8 - fee_asset1 - fee_asset2, 5)
        );
        assert_eq!(exchange.account().margin().position_margin(), 0.4);
        assert_eq!(
            round(exchange.account().margin().available_balance(), 2),
            round(0.4 - fee_asset1 - fee_asset2, 2)
        );
    }

    #[test]
    fn test_market_roundtrip() {
        let config = Config {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let fee_taker = config.fee_taker;
        let mut exchange = Exchange::new(config);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        let value = exchange.account().margin().available_balance() * 0.9;
        let size = exchange.ask * value;
        let buy_order = Order::market(Side::Buy, size).unwrap();
        let order_err = exchange.submit_order(buy_order);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let sell_order = Order::market(Side::Sell, size).unwrap();

        let order_err = exchange.submit_order(sell_order);
        assert!(order_err.is_ok());

        let fee_base = size * fee_taker;
        let fee_asset = fee_base / exchange.ask;

        exchange.check_orders();

        assert_eq!(exchange.account().position().size(), 0.0);
        assert_eq!(exchange.account().position().entry_price(), 1000.0);
        assert_eq!(exchange.account().position().value(), 0.0);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
        assert_eq!(
            exchange.account().margin().wallet_balance(),
            1.0 - 2.0 * fee_asset
        );
        assert_eq!(exchange.account().margin().position_margin(), 0.0);
        assert_eq!(
            exchange.account().margin().available_balance(),
            1.0 - 2.0 * fee_asset
        );

        let size = 900.0;
        let buy_order = Order::market(Side::Buy, size).unwrap();
        let order_err = exchange.submit_order(buy_order);
        assert!(order_err.is_ok());
        exchange.check_orders();

        let size = 950.0;
        let sell_order = Order::market(Side::Sell, size).unwrap();

        let order_err = exchange.submit_order(sell_order);
        assert!(order_err.is_ok());

        exchange.check_orders();

        assert_eq!(exchange.account().position().size(), -50.0);
        assert_eq!(exchange.account().position().entry_price(), 1000.0);
        assert_eq!(exchange.account().position().value(), 0.05);
        assert_eq!(exchange.account().position().unrealized_pnl(), 0.0);
        assert!(exchange.account().margin().wallet_balance() < 1.0);
        assert_eq!(exchange.account().margin().position_margin(), 0.05);
        assert!(exchange.account().margin().available_balance() < 1.0);
    }

    #[test]
    fn check_liquidation() {
        // TODO:
    }

    #[test]
    fn test_liquidate() {
        // TODO:
    }

    #[test]
    fn execute_limit() {
        let config = Config {
            fee_maker: 0.0,
            fee_taker: 0.001,
            starting_balance_base: 1.0,
            use_candles: false,
            leverage: 1.0,
        };
        let mut exchange = Exchange::new(config.clone());
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 0,
            price: 1000.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);

        let o: Order = Order::limit(Side::Buy, 900.0, 450.0).unwrap();
        exchange.submit_order(o).unwrap();
        assert_eq!(exchange.account().active_limit_orders().len(), 1);
        assert_eq!(exchange.account().margin().available_balance(), 0.5);
        assert_eq!(exchange.account().margin().order_margin(), 0.5);

        let t = Trade {
            timestamp: 1,
            price: 750.0,
            size: 100.0,
        };
        let (exec_orders, liq) = exchange.consume_trade(&t);
        assert!(!liq);
        assert_eq!(exec_orders.len(), 0);
        let t = Trade {
            timestamp: 1,
            price: 750.0,
            size: -100.0,
        };
        let (exec_orders, liq) = exchange.consume_trade(&t);
        assert!(!liq);
        assert_eq!(exec_orders.len(), 1);

        assert_eq!(exchange.bid, 750.0);
        assert_eq!(exchange.ask, 750.0);
        assert_eq!(exchange.account().active_limit_orders().len(), 0);
        assert_eq!(exchange.account().position().size(), 450.0);
        assert_eq!(exchange.account().position().value(), 0.6);
        assert_eq!(exchange.account().position().entry_price(), 900.0);
        assert_eq!(exchange.account().margin().wallet_balance(), 1.0);

        let o: Order = Order::limit(Side::Sell, 1000.0, 450.0).unwrap();
        let order_err = exchange.submit_order(o);
        assert!(order_err.is_ok());
        assert_eq!(exchange.account().active_limit_orders().len(), 1);

        let t = Trade {
            timestamp: 1,
            price: 1200.0,
            size: -100.0,
        };
        exchange.consume_trade(&t);
        let t = Trade {
            timestamp: 1,
            price: 1200.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);

        assert_eq!(exchange.account().active_limit_orders().len(), 0);
        assert_eq!(exchange.account().position().size(), 0.0);
        assert_eq!(exchange.account().margin().position_margin(), 0.0);
        assert_eq!(exchange.account().margin().wallet_balance(), 1.05);
        assert_eq!(exchange.account().margin().available_balance(), 1.05);

        let o: Order = Order::limit(Side::Sell, 1200.0, 600.0).unwrap();
        exchange.submit_order(o).unwrap();
        assert_eq!(exchange.account().active_limit_orders().len(), 1);
        let t = Trade {
            timestamp: 1,
            price: 1201.0,
            size: 100.0,
        };
        exchange.consume_trade(&t);
        assert_eq!(exchange.account().position().size(), -600.0);
        assert_eq!(round(exchange.account().position().value(), 1), 0.5);
        assert_eq!(round(exchange.account().margin().position_margin(), 1), 0.5);
        assert_eq!(round(exchange.account().margin().wallet_balance(), 2), 1.05);
        assert_eq!(
            round(exchange.account().margin().available_balance(), 2),
            0.55
        );
    }
}
