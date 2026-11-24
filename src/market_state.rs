use const_decimal::Decimal;
use getset::{
    CopyGetters,
    Getters,
    Setters,
};

use crate::{
    prelude::{
        Currency,
        MarketUpdate,
        Mon,
        PriceFilter,
        QuoteCurrency,
    },
    types::TimestampNs,
};

/// Some information regarding the state of the market.
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currency.
#[derive(Debug, Default, Clone, Getters, CopyGetters, Setters)]
pub struct MarketState<I, const D: u8>
where
    I: Mon<D>,
{
    /// The current bid
    #[getset(get_copy = "pub", set = "pub(crate)")]
    bid: QuoteCurrency<I, D>,

    /// The current ask
    #[getset(get_copy = "pub", set = "pub(crate)")]
    ask: QuoteCurrency<I, D>,

    /// The last trade price.
    #[getset(get_copy = "pub", set = "pub")]
    last_trade_price: QuoteCurrency<I, D>,

    /// Optional external fair / mark price (used when `MarkMethod::FairPrice` is configured).
    #[getset(get_copy = "pub", set = "pub")]
    fair_price: Option<QuoteCurrency<I, D>>,

    /// Optional external index price accompanying fair-price feeds.
    #[getset(get_copy = "pub", set = "pub")]
    index_price: Option<QuoteCurrency<I, D>>,

    /// The current timestamp in nanoseconds
    #[getset(get_copy = "pub")]
    current_ts_ns: TimestampNs,

    /// Used for synchronizing orders.
    #[getset(get_copy = "pub")]
    step: u64,
}

impl<I: Mon<D>, const D: u8> std::fmt::Display for MarketState<I, D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MarketState( bid: {}, ask: {}, ts_ns: {}, step: {} )",
            self.bid, self.ask, self.current_ts_ns, self.step
        )
    }
}

impl<I, const D: u8> MarketState<I, D>
where
    I: Mon<D>,
{
    /// Update the exchange state with new information
    ///
    /// ### Parameters:
    /// `market_update`: Newest market information
    /// `price_filter`: The pricing rules.
    ///
    #[inline]
    pub(crate) fn update_state<U, BaseOrQuote>(
        &mut self,
        market_update: &U,
        price_filter: &PriceFilter<I, D>,
    ) where
        U: MarketUpdate<I, D, BaseOrQuote>,
        BaseOrQuote: Currency<I, D>,
    {
        assert2::debug_assert!(market_update.validate_market_update(price_filter).is_ok());
        market_update.update_market_state(self);

        self.current_ts_ns = market_update.timestamp_exchange_ns();
        self.step += 1;
    }

    /// The best (highest) bid price.
    #[inline(always)]
    pub fn best_bid(&self) -> QuoteCurrency<I, D> {
        self.bid
    }

    /// The best (lowest) ask price.
    #[inline(always)]
    pub fn best_ask(&self) -> QuoteCurrency<I, D> {
        self.ask
    }

    /// Get the mid price
    #[inline(always)]
    pub fn mid_price(&self) -> QuoteCurrency<I, D> {
        (self.bid + self.ask) / Decimal::TWO
    }

    /// Resolve the contract's mark price according to `method`.
    ///
    /// - `MarkMethod::MidPrice`: `(bid + ask) / 2`.
    /// - `MarkMethod::BidAsk`: `(bid + ask) / 2` as a side-neutral reference
    ///   (use [`Self::mark_price_for_position`] for side-specific bid/ask marking).
    /// - `MarkMethod::FairPrice`: `fair_price` if set, falling back to `mid_price()`.
    #[inline]
    #[must_use]
    pub fn mark_price(
        &self,
        method: crate::contract_specification::MarkMethod,
    ) -> QuoteCurrency<I, D> {
        use crate::contract_specification::MarkMethod::*;
        match method {
            MidPrice | BidAsk => self.mid_price(),
            FairPrice => self.fair_price.unwrap_or_else(|| self.mid_price()),
        }
    }

    /// Resolve the mark price used to value or liquidate a position on `position_side`.
    ///
    /// - `MarkMethod::BidAsk`: `best_bid()` for longs, `best_ask()` for shorts,
    ///   `mid_price()` when neutral.
    /// - `MarkMethod::MidPrice`: `mid_price()`.
    /// - `MarkMethod::FairPrice`: `fair_price` if set, falling back to `mid_price()`.
    #[inline]
    #[must_use]
    pub fn mark_price_for_position(
        &self,
        method: crate::contract_specification::MarkMethod,
        position_side: crate::account::PositionSide,
    ) -> QuoteCurrency<I, D> {
        use crate::{
            account::PositionSide::*,
            contract_specification::MarkMethod::*,
        };
        match method {
            MidPrice => self.mid_price(),
            BidAsk => match position_side {
                Long => self.bid,
                Short => self.ask,
                Neutral => self.mid_price(),
            },
            FairPrice => self.fair_price.unwrap_or_else(|| self.mid_price()),
        }
    }

    /// Get the last observed timestamp in nanoseconts
    #[inline(always)]
    pub fn current_timestamp_ns(&self) -> TimestampNs {
        self.current_ts_ns
    }

    #[allow(missing_docs, reason = "test helper")]
    #[cfg(test)]
    pub fn from_components(
        bid: QuoteCurrency<I, D>,
        ask: QuoteCurrency<I, D>,
        last_trade_price: QuoteCurrency<I, D>,
        current_ts_ns: TimestampNs,
        step: u64,
    ) -> Self {
        Self {
            bid,
            ask,
            last_trade_price,
            fair_price: None,
            index_price: None,
            current_ts_ns,
            step,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        prelude::Bba,
        types::BaseCurrency,
    };

    #[test]
    fn market_state_display() {
        let state = MarketState::<i64, 1>::default();
        assert_eq!(
            &state.to_string(),
            "MarketState( bid: 0.0 Quote, ask: 0.0 Quote, ts_ns: 0, step: 0 )"
        );
    }

    #[test]
    fn market_state_best_bid_and_ask() {
        let mut state = MarketState::<i64, 1>::default();
        let pf = PriceFilter::default();
        state.update_state::<_, BaseCurrency<_, 1>>(
            &Bba {
                bid: QuoteCurrency::<i64, 1>::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 1.into(),
            },
            &pf,
        );
        assert_eq!(state.best_bid(), QuoteCurrency::new(100, 0));
        assert_eq!(state.best_ask(), QuoteCurrency::new(101, 0));
    }

    #[test]
    fn market_state_mid_price() {
        let mut state = MarketState::<i64, 1>::default();
        let pf = PriceFilter::default();
        state.update_state::<_, BaseCurrency<_, 1>>(
            &Bba {
                bid: QuoteCurrency::<i64, 1>::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 1.into(),
            },
            &pf,
        );
        assert_eq!(state.mid_price(), QuoteCurrency::new(1005, 1));
    }
}
