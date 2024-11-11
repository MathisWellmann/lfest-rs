use getset::{CopyGetters, Getters};

use crate::{
    contract_specification::ContractSpecification,
    prelude::{ConfigError, MarginCurrency, Mon, Position},
};

#[derive(Debug, Clone, Getters, CopyGetters)]
/// Define the Exchange configuration.
///
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currencies.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
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

    /// The position to start out with.
    #[getset(get = "pub")]
    starting_position: Position<I, D, BaseOrQuote::PairedCurrency>,

    /// The maximum number of open orders the user can have at any given time
    #[getset(get_copy = "pub")]
    max_num_open_orders: usize,

    /// The contract specification.
    #[getset(get = "pub")]
    contract_spec: ContractSpecification<I, D, BaseOrQuote::PairedCurrency>,

    /// The interval by which to sample the returns of user balances.
    /// This is used to analyze the trading performance later on, to enable things like `sharpe`, `sortino`, anything based on returns.
    #[getset(get_copy = "pub")]
    sample_returns_every_n_seconds: u64,
}

impl<I, const D: u8, BaseOrQuote> Config<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    /// Create a new Config.
    ///
    /// # Arguments:
    /// `starting_balance`: Initial Wallet Balance, denoted in QUOTE if using
    /// linear futures, denoted in BASE for inverse futures
    /// `max_num_open_orders`: The maximum number of open ordes a user can have
    /// at any time.
    /// `contract_specification`: More details on the actual contract traded.
    /// `sample_returns_every_n_seconds`: How often to sample the user balances for computing the returns.
    ///     Is used for computing for example the `sharpe` ratio or anything else that requires ln returns.
    ///
    /// # Returns:
    /// Either a valid `Config` or an Error
    pub fn new(
        starting_balance: BaseOrQuote,
        starting_position: Position<I, D, BaseOrQuote::PairedCurrency>,
        max_num_open_orders: usize,
        contract_specification: ContractSpecification<I, D, BaseOrQuote::PairedCurrency>,
        sample_returns_every_n_seconds: u64,
    ) -> Result<Self, ConfigError> {
        if max_num_open_orders == 0 {
            return Err(ConfigError::InvalidMaxNumOpenOrders);
        }
        if starting_balance <= BaseOrQuote::zero() {
            return Err(ConfigError::InvalidStartingBalance);
        }

        Ok(Config {
            starting_wallet_balance: starting_balance,
            starting_position,
            max_num_open_orders,
            contract_spec: contract_specification,
            sample_returns_every_n_seconds,
        })
    }
}
