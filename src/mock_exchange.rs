//! Convenince function used in tests.

use fpdec::{Dec, Decimal};

use crate::{
    account_tracker::{FullAccountTracker, NoAccountTracker},
    prelude::*,
};

/// The maker fee used in tests.
pub const TEST_FEE_MAKER: Fee = Fee::from_basis_points(2);
/// The taker fee used in tests.
pub const TEST_FEE_TAKER: Fee = Fee::from_basis_points(6);

/// Constructs a mock exchange (for linear futures) for testing.
/// The size is denoted in `BaseCurrency`
/// and the margin currency is `QuoteCurency`
pub fn mock_exchange_linear(
) -> Exchange<NoAccountTracker, BaseCurrency, (), InMemoryTransactionAccounting<QuoteCurrency>> {
    let acc_tracker = NoAccountTracker;
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Dec!(0.5),
        PriceFilter::default(),
        QuantityFilter::new(None, None, base!(0.01)).unwrap(),
        TEST_FEE_MAKER,
        TEST_FEE_TAKER,
    )
    .expect("works");
    let config = Config::new(quote!(1000), 200, contract_spec, 3600).unwrap();
    Exchange::new(acc_tracker, config)
}

/// Constructs a mock exchange (for linear futures) for testing.
/// The size is denoted in `BaseCurrency`
/// and the margin currency is `QuoteCurency`
pub fn mock_exchange_linear_with_account_tracker(
    starting_balance: QuoteCurrency,
) -> Exchange<
    FullAccountTracker<QuoteCurrency>,
    BaseCurrency,
    (),
    InMemoryTransactionAccounting<QuoteCurrency>,
> {
    let acc_tracker = FullAccountTracker::new(starting_balance);
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Dec!(0.5),
        PriceFilter::default(),
        QuantityFilter::new(None, None, base!(0.01)).unwrap(),
        TEST_FEE_MAKER,
        TEST_FEE_TAKER,
    )
    .expect("works");
    let config = Config::new(starting_balance, 200, contract_spec, 3600).unwrap();
    Exchange::new(acc_tracker, config)
}

/// Constructs a mock exchange (for inverse futures) for testing.
pub fn mock_exchange_inverse(
    starting_balance: BaseCurrency,
) -> Exchange<NoAccountTracker, QuoteCurrency, (), InMemoryTransactionAccounting<BaseCurrency>> {
    let acc_tracker = NoAccountTracker;
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Dec!(0.5),
        PriceFilter::default(),
        QuantityFilter::default(),
        TEST_FEE_MAKER,
        TEST_FEE_TAKER,
    )
    .expect("works");
    let config = Config::new(starting_balance, 200, contract_spec, 3600).unwrap();
    Exchange::new(acc_tracker, config)
}

/// Mocks `TransactionAccounting` for testing purposes.
#[derive(Default, Clone)]
pub struct MockTransactionAccounting;

impl<M> TransactionAccounting<M> for MockTransactionAccounting
where
    M: MarginCurrency,
{
    fn new(_user_starting_wallet_balance: M) -> Self {
        Self {}
    }

    fn create_margin_transfer(&mut self, _transaction: Transaction<M>) -> Result<()> {
        Ok(())
    }

    fn margin_balance_of(&self, _account: AccountId) -> Result<M> {
        Ok(M::new_zero())
    }
}
