use crate::{
    market_state::MarketState,
    order_margin::OrderMarginOnline,
    prelude::Position,
    types::{Currency, LimitOrder, MarginCurrency, MarketOrder, Pending, QuoteCurrency},
};

/// The error that the `RiskEngine` outputs, if any.
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum RiskError {
    #[error("The `Trader` does not have enough balance.")]
    NotEnoughAvailableBalance,

    #[error("The position will be liquidated!")]
    Liquidate,
}

pub(crate) trait RiskEngine<M, UserOrderId>
where
    M: Currency + MarginCurrency,
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
        position: &Position<M::PairedCurrency>,
        position_margin: M,
        order: &MarketOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
        fill_price: QuoteCurrency,
        available_wallet_balance: M,
    ) -> Result<(), RiskError>;

    /// Checks if the account it able to satisfy the margin requirements for a new limit order.
    fn check_limit_order(
        &self,
        position: &Position<M::PairedCurrency>,
        position_margin: M,
        order: &LimitOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
        available_wallet_balance: M,
        order_margin: &OrderMarginOnline<M::PairedCurrency, UserOrderId>,
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
        market_state: &MarketState,
        position: &Position<M::PairedCurrency>,
    ) -> Result<(), RiskError>;
}
