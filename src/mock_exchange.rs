//! Convenince function used in tests.

use const_decimal::Decimal;

use crate::{
    account_tracker::{FullAccountTracker, NoAccountTracker},
    prelude::*,
    utils::NoUserOrderId,
};

/// The constant decimal precision.
pub const DECIMALS: u8 = 5;

/// The maker fee used in tests.
pub fn test_fee_maker() -> Fee<i64, DECIMALS, Maker> {
    Fee::from(Decimal::try_from_scaled(2, 4).unwrap())
}

/// The taker fee used in tests.
pub fn test_fee_taker() -> Fee<i64, DECIMALS, Taker> {
    Fee::from(Decimal::try_from_scaled(6, 4).unwrap())
}

/// Constructs a mock exchange (for linear futures) for testing.
/// The size is denoted in `BaseCurrency`
/// and the margin currency is `QuoteCurency`
pub fn mock_exchange_linear() -> Exchange<
    i64,
    DECIMALS,
    BaseCurrency<i64, DECIMALS>,
    NoUserOrderId,
    InMemoryTransactionAccounting<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>>,
    NoAccountTracker,
> {
    let acc_tracker = NoAccountTracker;
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Decimal::try_from_scaled(5, 1).unwrap(),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).unwrap(),
        test_fee_maker(),
        test_fee_taker(),
    )
    .expect("works");
    let config = Config::new(QuoteCurrency::new(1000, 0), 10, contract_spec, 3600).unwrap();
    Exchange::new(acc_tracker, config)
}

/// Constructs a mock exchange (for linear futures) for testing.
/// The size is denoted in `BaseCurrency`
/// and the margin currency is `QuoteCurency`
pub fn mock_exchange_linear_with_account_tracker(
    starting_balance: QuoteCurrency<i64, DECIMALS>,
) -> Exchange<
    i64,
    DECIMALS,
    BaseCurrency<i64, DECIMALS>,
    NoUserOrderId,
    InMemoryTransactionAccounting<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>>,
    FullAccountTracker<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>>,
> {
    let acc_tracker = FullAccountTracker::new(starting_balance);
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Decimal::try_from_scaled(5, 1).unwrap(),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).unwrap(),
        test_fee_maker(),
        test_fee_taker(),
    )
    .expect("works");
    let config = Config::new(starting_balance, 200, contract_spec, 3600).unwrap();
    Exchange::new(acc_tracker, config)
}

/// Constructs a mock exchange (for inverse futures) for testing.
pub fn mock_exchange_inverse(
    starting_balance: BaseCurrency<i64, DECIMALS>,
) -> Exchange<
    i64,
    DECIMALS,
    QuoteCurrency<i64, DECIMALS>,
    NoUserOrderId,
    InMemoryTransactionAccounting<i64, DECIMALS, BaseCurrency<i64, DECIMALS>>,
    NoAccountTracker,
> {
    let acc_tracker = NoAccountTracker;
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Decimal::try_from_scaled(5, 1).expect("works"),
        PriceFilter::default(),
        QuantityFilter::default(),
        test_fee_maker(),
        test_fee_taker(),
    )
    .expect("works");
    let config = Config::new(starting_balance, 200, contract_spec, 3600).unwrap();
    Exchange::new(acc_tracker, config)
}

/// Mocks `TransactionAccounting` for testing purposes.
#[derive(Default, Clone)]
pub struct MockTransactionAccounting;

impl<I, const D: u8, BaseOrQuote> TransactionAccounting<I, D, BaseOrQuote>
    for MockTransactionAccounting
where
    I: Mon<D>,
    BaseOrQuote: MarginCurrency<I, D>,
{
    fn new(_user_starting_wallet_balance: BaseOrQuote) -> Self {
        Self {}
    }

    fn create_margin_transfer(
        &mut self,
        _transaction: Transaction<I, D, BaseOrQuote>,
    ) -> Result<()> {
        Ok(())
    }

    fn margin_balance_of(&self, _account: AccountId) -> Result<BaseOrQuote> {
        Ok(BaseOrQuote::zero())
    }
}
