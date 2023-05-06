use crate::{
    clearing_house::ClearingHouse,
    market_state::MarketState,
    prelude::{Account, AccountTracker},
    types::{Currency, MarginCurrency, Order, QuoteCurrency, Side},
};

/// Component that facilitates the execution of trades on behalf of traders and market participants.
/// The `ExecutionEngine`,  is responsible for executing trades that have been matched by the `MatchingEngine`.
/// Once the `MatchingEngine` has found a suitable counterparty for a trade,
/// the ExecutionEngine takes over and executes the trade by sending the relevant trade details to the `ClearingHosue`,
/// which **guarantees** the settlement of the trade.
#[derive(Debug, Clone, Default)]
pub(crate) struct ExecutionEngine<A, S> {
    __marker: std::marker::PhantomData<(A, S)>,
}

impl<A, S> ExecutionEngine<A, S>
where
    A: AccountTracker<S::PairedCurrency>,
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
    pub(crate) fn new() -> Self {
        Self {
            __marker: std::marker::PhantomData,
        }
    }

    pub(crate) fn execute_market_order(
        &mut self,
        account: &mut Account<S>,
        market_state: &MarketState,
        order: &Order<S>,
        clearing_house: &ClearingHouse<A, S::PairedCurrency>,
    ) {
        match order.side() {
            Side::Buy => {
                self.execute_market_buy(account, market_state, order.quantity(), market_state.ask())
            }
            Side::Sell => self.execute_market_sell(
                account,
                market_state,
                order.quantity(),
                market_state.bid(),
            ),
        }
    }

    fn execute_market_buy(
        &self,
        account: &mut Account<S>,
        market_state: &MarketState,
        quantity: S,
        price: QuoteCurrency,
    ) {
        if account.position().size() >= S::new_zero() {
            // account.try_increase_long(quantity, price);
        } else {
            if quantity > account.position().size().abs() {
                // account.try_turn_around_short(quantity, price);
            } else {
                // decrease short and realize pnl.
                // account
                //     .try_decrease_short(
                //         quantity,
                //         price,
                //         self.config.fee_taker(),
                //         market_state.current_timestamp_ns(),
                //     )
                //     .expect("Must be valid; qed");
            }
        }
        todo!()
    }

    fn execute_market_sell(
        &self,
        account: &mut Account<S>,
        market_state: &MarketState,
        quantity: S,
        price: QuoteCurrency,
    ) {
        if account.position().size() >= S::new_zero() {
            if quantity > account.position().size() {
                // account.try_turn_around_long(quantity, price);
            } else {
                // decrease_long and realize pnl.
                // account.try_decrease_long(
                //     quantity,
                //     price,
                //     self.config.fee_taker(),
                //     market_state.current_timestamp_ns(),
                // );
            }
        } else {
            // account.try_increase_short(quantity, price);
        }
        todo!()
    }

    // TODO: Is there even a need to differentiate between limit and market here?
    fn execute_limit_buy(
        &self,
        account: &mut Account<S>,
        market_state: &MarketState,
        quantity: S,
        price: QuoteCurrency,
    ) {
        todo!()
    }

    fn execute_limit_sell(
        &self,
        account: &mut Account<S>,
        market_state: &MarketState,
        quantity: S,
        price: QuoteCurrency,
    ) {
        todo!()
    }
}
