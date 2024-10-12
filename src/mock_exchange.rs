//! Convenince function used in tests.

use fpdec::{Dec, Decimal};
use num_traits::Zero;

use crate::{
    account_tracker::{FullAccountTracker, NoAccountTracker},
    prelude::*,
};

/// The maker fee used in tests.
pub const TEST_FEE_MAKER: Fee<Maker> = Fee::from_basis_points(2);
/// The taker fee used in tests.
pub const TEST_FEE_TAKER: Fee<Taker> = Fee::from_basis_points(6);

/// Constructs a mock exchange (for linear futures) for testing.
/// The size is denoted in `BaseCurrency`
/// and the margin currency is `QuoteCurency`
pub fn mock_exchange_linear(
) -> Exchange<NoAccountTracker, Decimal, Base, (), InMemoryTransactionAccounting<Decimal, Quote>> {
    let acc_tracker = NoAccountTracker;
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Dec!(0.5),
        PriceFilter::default(),
        QuantityFilter::new(None, None, Monies::new(Dec!(0.01))).unwrap(),
        TEST_FEE_MAKER,
        TEST_FEE_TAKER,
    )
    .expect("works");
    let config = Config::new(Monies::new(Dec!(1000)), 200, contract_spec, 3600).unwrap();
    Exchange::new(acc_tracker, config)
}

/// Constructs a mock exchange (for linear futures) for testing.
/// The size is denoted in `BaseCurrency`
/// and the margin currency is `QuoteCurency`
pub fn mock_exchange_linear_with_account_tracker(
    starting_balance: Monies<Decimal, Quote>,
) -> Exchange<
    FullAccountTracker<Decimal, Quote>,
    Decimal,
    Base,
    (),
    InMemoryTransactionAccounting<Decimal, Quote>,
> {
    let acc_tracker = FullAccountTracker::new(starting_balance);
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Dec!(0.5),
        PriceFilter::default(),
        QuantityFilter::new(None, None, Monies::new(Dec!(0.01))).unwrap(),
        TEST_FEE_MAKER,
        TEST_FEE_TAKER,
    )
    .expect("works");
    let config = Config::new(starting_balance, 200, contract_spec, 3600).unwrap();
    Exchange::new(acc_tracker, config)
}

/// Constructs a mock exchange (for inverse futures) for testing.
pub fn mock_exchange_inverse(
    starting_balance: Monies<Decimal, Base>,
) -> Exchange<NoAccountTracker, Decimal, Quote, (), InMemoryTransactionAccounting<Decimal, Base>> {
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

impl<T, BaseOrQuote> TransactionAccounting<T, BaseOrQuote> for MockTransactionAccounting
where
    T: Mon,
    BaseOrQuote: MarginCurrencyMarker<T>,
{
    fn new(_user_starting_wallet_balance: Monies<T, BaseOrQuote>) -> Self {
        Self {}
    }

    fn create_margin_transfer(
        &mut self,
        _transaction: Transaction<T, BaseOrQuote>,
    ) -> Result<(), T> {
        Ok(())
    }

    fn margin_balance_of(&self, _account: AccountId) -> Result<Monies<T, BaseOrQuote>, T> {
        Ok(Monies::zero())
    }
}
