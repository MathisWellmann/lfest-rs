use crate::{
    market_state::MarketState,
    order_margin::OrderMargin,
    prelude::{Currency, Mon, Position, QuoteCurrency, RiskError},
    types::{LimitOrder, MarginCurrency, MarketOrder, Pending},
};

pub(crate) trait RiskEngine<I, const D: u8, BaseOrQuote, UserOrderId>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderId: Clone + std::fmt::Debug + Eq + PartialEq + std::hash::Hash + Default,
{
    /// Checks if the account it able to satisfy the margin requirements for a new market order.
    ///
    /// When a trader submits an order to increase their position,
    /// the risk engine will typically calculate the margin requirements as if the new order is executed and added to their existing positions.
    /// The risk engine will consider the notional value of the new order, the current market price,
    /// and the leverage used to determine the required margin for the entire position.
    ///
    /// On the other hand, when a trader submits an order to decrease their position,
    /// the risk engine will typically calculate the margin requirements as if the order is executed and reduces the size of their existing position.
    /// The risk engine will consider the notional value of the order, the current market price,
    /// and the leverage used to determine the new required margin for the remaining position.
    ///
    /// # Returns:
    /// If Err, the account cannot satisfy the margin requirements.
    fn check_market_order(
        &self,
        position: &Position<I, D, BaseOrQuote>,
        position_margin: BaseOrQuote::PairedCurrency,
        order: &MarketOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>,
        fill_price: QuoteCurrency<I, D>,
        available_wallet_balance: BaseOrQuote::PairedCurrency,
    ) -> Result<(), RiskError>;

    /// Checks if the account it able to satisfy the margin requirements for a new limit order.
    fn check_limit_order(
        &self,
        position: &Position<I, D, BaseOrQuote>,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderId, Pending<I, D, BaseOrQuote>>,
        available_wallet_balance: BaseOrQuote::PairedCurrency,
        order_margin: &OrderMargin<I, D, BaseOrQuote, UserOrderId>,
    ) -> Result<(), RiskError>;

    /// Ensure the account has enough maintenance margin, to keep the position open.
    /// The maintenance margin is the minimum amount of funds that must be maintained in a trader's account
    /// to ensure that they can meet any losses that may occur due to adverse price movements in the futures contract.
    ///
    /// # Arguments:
    /// `market_state`: The current market information.
    /// `account`: The user account.
    ///
    /// # Returns:
    /// If Err, the account must be liquidated.
    fn check_maintenance_margin(
        &self,
        market_state: &MarketState<I, D>,
        position: &Position<I, D, BaseOrQuote>,
    ) -> Result<(), RiskError>;
}
