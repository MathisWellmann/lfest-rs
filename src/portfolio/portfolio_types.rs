//! Event, settlement, and error types for [`crate::portfolio::PortfolioExchange`].

use std::fmt::Display;

use thiserror::Error;

use crate::{
    account::{
        ActiveLimitOrders,
        Position,
    },
    contract_specification::ContractSpecification,
    market_state::MarketState,
    order_rate_limiter::RateLimitReached,
    types::{
        AmendLimitOrderError,
        CancelLimitOrderError,
        Currency,
        Filled,
        LimitOrder,
        LimitOrderEvent,
        MarginCurrency,
        MarketId,
        MarketOrder,
        MarketStatus,
        Mon,
        Pending,
        Solvency,
        SubmitLimitOrderError,
        SubmitMarketOrderError,
        UserOrderId,
    },
};

/// A limit order event tagged with the [`MarketId`] on which it occurred.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PortfolioOrderEvent<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D> + Display,
    BaseOrQuote: Currency<I, D> + Display,
    UserOrderIdT: UserOrderId + Display,
{
    /// The market where this event occurred.
    pub market_id: MarketId,
    /// The fill or forced cancellation event.
    pub event: LimitOrderEvent<I, D, BaseOrQuote, UserOrderIdT>,
}

/// A pending limit order alias used across portfolio APIs.
pub type PendingLimitOrder<I, const D: u8, BaseOrQuote, UserOrderIdT> =
    LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>;

/// Resting limit order force-cancelled during a portfolio margin call or liquidation,
/// tagged with its [`MarketId`].
pub type TaggedForcedCancels<I, const D: u8, BaseOrQuote, UserOrderIdT> =
    Vec<(MarketId, PendingLimitOrder<I, D, BaseOrQuote, UserOrderIdT>)>;

/// Atomic result of settling a market order on [`crate::portfolio::PortfolioExchange`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PortfolioMarketOrderSettlement<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// The market on which the order executed.
    pub market_id: MarketId,
    /// The filled market order.
    pub filled_order: MarketOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>,
    /// Resting orders across any market force-cancelled during collateral reconciliation.
    pub forced_cancels: TaggedForcedCancels<I, D, BaseOrQuote, UserOrderIdT>,
    /// Portfolio solvency after settlement and reconciliation.
    pub solvency: Solvency,
}

/// Atomic result of settling perpetual funding on one market of [`crate::portfolio::PortfolioExchange`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PortfolioFundingSettlement<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// The market where funding was applied.
    pub market_id: MarketId,
    /// Signed nominal funding cashflow in margin currency.
    pub cashflow: BaseOrQuote::PairedCurrency,
    /// Resting orders across any market force-cancelled during collateral reconciliation.
    pub forced_cancels: TaggedForcedCancels<I, D, BaseOrQuote, UserOrderIdT>,
    /// Portfolio solvency after settlement and reconciliation.
    pub solvency: Solvency,
}

/// Atomic result of delisting a market via [`crate::portfolio::PortfolioExchange::delist_market`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DelistSettlement<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// The delisted market.
    pub market_id: MarketId,
    /// Signed position quantity that was settled (zero if neutral).
    pub settled_quantity: BaseOrQuote,
    /// Realized PnL credited/debited to the shared wallet (zero if neutral).
    pub realized_pnl: BaseOrQuote::PairedCurrency,
    /// Every resting limit order on the delisted market that was cancelled.
    pub cancelled_orders:
        Vec<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>>,
    /// Additional forced cancellations on other markets if delist losses triggered a margin call.
    pub forced_cancels: TaggedForcedCancels<I, D, BaseOrQuote, UserOrderIdT>,
    /// Portfolio solvency after settlement and reconciliation.
    pub solvency: Solvency,
}

/// Read-only snapshot reference to a single market's state inside [`crate::portfolio::PortfolioExchange`].
#[derive(Debug)]
pub struct MarketSlotView<'slot, I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Dense market identifier.
    pub market_id: MarketId,
    /// Current lifecycle status.
    pub status: MarketStatus,
    /// Contract specification.
    pub contract_spec: &'slot ContractSpecification<I, D, BaseOrQuote>,
    /// Current market quotes and mark price state.
    pub market_state: &'slot MarketState<I, D>,
    /// Current open position on this market.
    pub position: &'slot Position<I, D, BaseOrQuote>,
    /// Current active resting limit orders on this market.
    pub active_limit_orders: &'slot ActiveLimitOrders<I, D, BaseOrQuote, UserOrderIdT>,
}

/// Errors specific to multi-symbol [`crate::portfolio::PortfolioExchange`] operations.
#[derive(Debug, Clone, Error, Eq, PartialEq)]
#[allow(missing_docs, reason = "Self documenting")]
pub enum PortfolioError<UserOrderIdT> {
    #[error("unknown market id {0}")]
    UnknownMarket(MarketId),

    #[error("market {0} is delisted and no longer accepts orders")]
    MarketDelisted(MarketId),

    #[error("market {0} is in reduce-only mode and rejects risk-increasing orders")]
    MarketReduceOnly(MarketId),

    #[error("maximum number of markets ({0}) reached")]
    MaxMarketsReached(usize),

    #[error(transparent)]
    RateLimitReached(#[from] RateLimitReached),

    #[error(transparent)]
    SubmitMarketOrder(#[from] SubmitMarketOrderError),

    #[error(transparent)]
    SubmitLimitOrder(#[from] SubmitLimitOrderError),

    #[error(transparent)]
    AmendLimitOrder(#[from] AmendLimitOrderError),

    #[error(transparent)]
    CancelLimitOrder(#[from] CancelLimitOrderError<UserOrderIdT>),
}
