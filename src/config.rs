use getset::{CopyGetters, Getters};

use crate::{
    contract_specification::ContractSpecification,
    types::{Currency, Error, Result},
};

#[derive(Debug, Clone, Getters, CopyGetters)]
/// Define the Exchange configuration
pub struct Config<M>
where
    M: Currency,
{
    /// The starting balance of account (denoted in margin currency).
    /// The concrete `Currency` here defines the futures type.
    /// If `QuoteCurrency` is used as the margin currency,
    /// then its a linear futures contract.
    /// If `BaseCurrency` is used as the margin currency,
    /// then its an inverse futures contract.
    #[getset(get_copy = "pub")]
    starting_wallet_balance: M,

    /// The maximum number of open orders the user can have at any given time
    #[getset(get_copy = "pub")]
    max_num_open_orders: usize,

    /// The contract specification.
    #[getset(get = "pub")]
    contract_spec: ContractSpecification<M::PairedCurrency>,
}

impl<M> Config<M>
where
    M: Currency,
{
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
    #[allow(clippy::complexity)]
    pub fn new(
        starting_balance: M,
        max_num_open_orders: usize,
        contract_specification: ContractSpecification<M::PairedCurrency>,
    ) -> Result<Self> {
        if max_num_open_orders == 0 {
            return Err(Error::InvalidMaxNumOpenOrders);
        }
        if starting_balance <= M::new_zero() {
            return Err(Error::InvalidStartingBalance);
        }

        Ok(Config {
            starting_wallet_balance: starting_balance,
            max_num_open_orders,
            contract_spec: contract_specification,
        })
    }
}
