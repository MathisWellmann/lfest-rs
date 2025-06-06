use std::num::NonZeroUsize;

use getset::{CopyGetters, Getters};

use crate::{
    contract_specification::ContractSpecification,
    prelude::{ConfigError, MarginCurrency, Mon},
    types::OrderRateLimits,
};

/// Define the Exchange configuration.
///
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currencies.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
#[derive(Debug, Clone, Getters, CopyGetters)]
pub struct Config<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    /// The starting balance of account (denoted in margin currency).
    /// The concrete `Currency` here defines the futures type.
    /// If `QuoteCurrency` is used as the margin currency,
    /// then its a linear futures contract.
    /// If `BaseCurrency` is used as the margin currency,
    /// then its an inverse futures contract.
    #[getset(get_copy = "pub")]
    starting_wallet_balance: BaseOrQuote,

    /// The maximum number of open orders the user can have at any given time.
    #[getset(get_copy = "pub")]
    max_num_open_orders: NonZeroUsize,

    /// The contract specification.
    #[getset(get = "pub")]
    contract_spec: ContractSpecification<I, D, BaseOrQuote::PairedCurrency>,

    /// The submission rate limits for orders.
    #[getset(get = "pub")]
    order_rate_limits: OrderRateLimits,
}

impl<I, const D: u8, BaseOrQuote> Config<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    // TODO: use `typed-builder`
    /// Create a new Config.
    ///
    /// # Arguments:
    /// `starting_balance`: Initial Wallet Balance, denoted in QUOTE if using
    /// linear futures, denoted in BASE for inverse futures
    /// `max_num_open_orders`: The maximum number of open ordes a user can have
    /// at any time.
    /// `contract_specification`: More details on the actual contract traded.
    ///
    /// # Returns:
    /// Either a valid `Config` or an Error
    pub fn new(
        starting_balance: BaseOrQuote,
        max_num_open_orders: NonZeroUsize,
        contract_specification: ContractSpecification<I, D, BaseOrQuote::PairedCurrency>,
        order_rate_limits: OrderRateLimits,
    ) -> Result<Self, ConfigError> {
        if starting_balance <= BaseOrQuote::zero() {
            return Err(ConfigError::InvalidStartingBalance);
        }

        Ok(Config {
            starting_wallet_balance: starting_balance,
            max_num_open_orders,
            contract_spec: contract_specification,
            order_rate_limits,
        })
    }
}
