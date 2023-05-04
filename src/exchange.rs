use fpdec::Decimal;

use crate::{
    account::Account,
    account_tracker::AccountTracker,
    clearing_house::ClearingHouse,
    config::Config,
    errors::{Error, OrderError},
    prelude::Side,
    quote,
    risk_engine::IsolatedMarginRiskEngine,
    types::{Currency, MarginCurrency, MarketUpdate, Order, OrderType, QuoteCurrency},
};

#[derive(Debug, Clone)]
/// The main leveraged futures exchange for simulated trading
pub struct Exchange<A, S>
where
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
    config: Config<S::PairedCurrency>,
    clearing_house: ClearingHouse<A, S, IsolatedMarginRiskEngine<S::PairedCurrency>>,
    // TODO: encapsulate all of the market state into `MarketState` or similar.
    bid: QuoteCurrency,
    ask: QuoteCurrency,
    step: u64, // used for synchronizing orders
    high: QuoteCurrency,
    low: QuoteCurrency,
    // The current timestamp in nanoseconds
    current_ts_ns: i64,
}

impl<A, S> Exchange<A, S>
where
    A: AccountTracker<S::PairedCurrency>,
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
    /// Create a new Exchange with the desired config and whether to use candles
    /// as infomation source
    pub fn new(account_tracker: A, config: Config<S::PairedCurrency>) -> Self {
        let account = Account::new(config.starting_balance(), config.fee_taker());
        let risk_engine = IsolatedMarginRiskEngine::<S::PairedCurrency>::new(
            config.contract_specification().clone(),
        );
        let clearing_house = ClearingHouse::new(risk_engine, account_tracker, account);

        Self {
            config,
            bid: quote!(0.0),
            ask: quote!(0.0),
            next_order_id: 0,
            step: 0,
            high: quote!(0.0),
            low: quote!(0.0),
            current_ts_ns: 0,
            clearing_house,
        }
    }

    /// Return a reference to current exchange config
    #[inline(always)]
    pub fn config(&self) -> &Config<S::PairedCurrency> {
        &self.config
    }

    /// Return the bid price
    #[inline(always)]
    pub fn bid(&self) -> QuoteCurrency {
        self.bid
    }

    /// Return the ask price
    #[inline(always)]
    pub fn ask(&self) -> QuoteCurrency {
        self.ask
    }

    /// Return the current time step
    #[inline(always)]
    pub fn current_step(&self) -> u64 {
        self.step
    }

    /// Return a reference to Account
    #[inline(always)]
    pub fn account(&self) -> &Account<S> {
        &self.clearing_house.user_account()
    }

    /// Return a mutable reference to Account
    #[inline(always)]
    pub fn account_mut(&mut self) -> &mut Account<S> {
        &mut self.clearing_house.user_account_mut()
    }

    /// Update the exchange state with new information
    ///
    /// ### Parameters:
    /// `timestamp_ns`: Is used in the AccountTracker `A`
    ///     and if setting order timestamps is enabled in the config.
    /// `market_update`: Newest market information
    ///
    /// ### Returns:
    /// executed orders
    /// true if position has been liquidated
    pub fn update_state(
        &mut self,
        timestamp_ns: u64,
        market_update: MarketUpdate,
    ) -> Result<(Vec<Order<S>>, bool), Error> {
        self.config
            .price_filter()
            .validate_market_update(&market_update)?;
        match market_update {
            MarketUpdate::Bba { bid, ask } => {
                self.bid = bid;
                self.ask = ask;
                self.high = ask;
                self.low = bid;
            }
            MarketUpdate::Candle {
                bid,
                ask,
                high,
                low,
            } => {
                self.bid = bid;
                self.ask = ask;
                self.high = high;
                self.low = low;
            }
        }
        self.current_ts_ns = timestamp_ns as i64;

        todo!("risk engine checks margin");

        self.check_orders();

        // self.user_account.update(self.bid, self.ask, timestamp_ns);

        self.step += 1;

        Ok((self.clearing_house.executed_orders(), false))
    }
}
