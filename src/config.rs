use crate::{
    contract_specification::ContractSpecification,
    types::{Currency, Error, Leverage, Result},
};

#[derive(Debug, Clone)]
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
    starting_balance: M,
    /// The maximum number of open orders the user can have at any given time
    max_num_open_orders: usize,
    initial_leverage: Leverage,
    /// The contract specification.
    contract_specification: ContractSpecification<M::PairedCurrency>,
}

impl<M> Config<M>
where
    M: Currency,
{
    /// Create a new Config.
    ///
    /// # Arguments:
    /// `fee_maker`: The maker fee as fraction, e.g 6bp -> 0.0006
    /// `fee_taker`: The taker fee as fraction
    /// `starting_balance`: Initial Wallet Balance, denoted in QUOTE if using
    /// linear futures, denoted in BASE for inverse futures
    /// `max_num_open_orders`: The maximum number of open ordes a user can have
    /// at any time.
    /// `initial_leverage`: The initial desired leverage of positions.
    /// `contract_specification`: More details on the actual contract traded.
    ///
    /// # Returns:
    /// Either a valid Config or an Error
    #[allow(clippy::complexity)]
    pub fn new(
        starting_balance: M,
        max_num_open_orders: usize,
        initial_leverage: Leverage,
        contract_specification: ContractSpecification<M::PairedCurrency>,
    ) -> Result<Self> {
        if max_num_open_orders == 0 {
            return Err(Error::InvalidMaxNumOpenOrders);
        }
        if starting_balance <= M::new_zero() {
            return Err(Error::InvalidStartingBalance);
        }

        Ok(Config {
            starting_balance,
            max_num_open_orders,
            initial_leverage,
            contract_specification,
        })
    }

    /// Return the starting wallet balance of this Config
    #[inline(always)]
    pub fn starting_balance(&self) -> M {
        self.starting_balance
    }

    /// Returns the maximum number of open orders that are allowed
    #[inline(always)]
    pub fn max_num_open_orders(&self) -> usize {
        self.max_num_open_orders
    }

    /// Return a reference to the `ContractSpecification`
    #[inline(always)]
    pub fn contract_specification(&self) -> &ContractSpecification<M::PairedCurrency> {
        &self.contract_specification
    }

    /// Return the configured initial leverage
    #[inline(always)]
    pub fn initial_leverage(&self) -> Leverage {
        self.initial_leverage
    }
}
