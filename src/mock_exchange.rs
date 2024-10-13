//! Convenince function used in tests.

use const_decimal::Decimal;

use crate::{
    account_tracker::{FullAccountTracker, NoAccountTracker},
    prelude::*,
};

/// The maker fee used in tests.
pub const TEST_FEE_MAKER: Fee<Maker> = Fee::from_basis_points(2);
/// The taker fee used in tests.
pub const TEST_FEE_TAKER: Fee<Taker> = Fee::from_basis_points(6);

const DB: u8 = 4;
const DQ: u8 = 2;

/// Constructs a mock exchange (for linear futures) for testing.
/// The size is denoted in `BaseCurrency`
/// and the margin currency is `QuoteCurency`
pub fn mock_exchange_linear() -> Exchange<
    i32,
    DB,
    DQ,
    BaseCurrency<i32, DB, DQ>,
    (),
    InMemoryTransactionAccounting<i32, DB, DQ, QuoteCurrency<i32, DB, DQ>>,
    NoAccountTracker,
> {
    let acc_tracker = NoAccountTracker;
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        BasisPointFrac::from(Decimal::try_from_scaled(5, 1).unwrap()),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).unwrap(),
        TEST_FEE_MAKER,
        TEST_FEE_TAKER,
    )
    .expect("works");
    let config = Config::new(QuoteCurrency::new(1000, 0), 200, contract_spec, 3600).unwrap();
    Exchange::new(acc_tracker, config)
}

/// Constructs a mock exchange (for linear futures) for testing.
/// The size is denoted in `BaseCurrency`
/// and the margin currency is `QuoteCurency`
pub fn mock_exchange_linear_with_account_tracker(
    starting_balance: QuoteCurrency<i32, DB, DQ>,
) -> Exchange<
    i32,
    DB,
    DQ,
    BaseCurrency<i32, DB, DQ>,
    (),
    InMemoryTransactionAccounting<i32, DB, DQ, QuoteCurrency<i32, DB, DQ>>,
    FullAccountTracker<i32, DB, DQ, QuoteCurrency<i32, DB, DQ>>,
> {
    let acc_tracker = FullAccountTracker::new(starting_balance);
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        BasisPointFrac::from(Decimal::try_from_scaled(5, 1).unwrap()),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).unwrap(),
        TEST_FEE_MAKER,
        TEST_FEE_TAKER,
    )
    .expect("works");
    let config = Config::new(starting_balance, 200, contract_spec, 3600).unwrap();
    Exchange::new(acc_tracker, config)
}

/// Constructs a mock exchange (for inverse futures) for testing.
pub fn mock_exchange_inverse(
    starting_balance: BaseCurrency<i32, DB, DQ>,
) -> Exchange<
    i32,
    DB,
    DQ,
    QuoteCurrency<i32, DB, DQ>,
    (),
    InMemoryTransactionAccounting<i32, DB, DQ, BaseCurrency<i32, DB, DQ>>,
    NoAccountTracker,
> {
    let acc_tracker = NoAccountTracker;
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        BasisPointFrac::from(Decimal::try_from_scaled(50, 1).expect("works")),
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

impl<I, const DB: u8, const DQ: u8, BaseOrQuote> TransactionAccounting<I, DB, DQ, BaseOrQuote>
    for MockTransactionAccounting
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: MarginCurrencyMarker<I, DB, DQ>,
{
    fn new(_user_starting_wallet_balance: BaseOrQuote) -> Self {
        Self {}
    }

    fn create_margin_transfer(
        &mut self,
        _transaction: Transaction<I, DB, DQ, BaseOrQuote>,
    ) -> Result<(), I, DB, DQ> {
        Ok(())
    }

    fn margin_balance_of(&self, _account: AccountId) -> Result<BaseOrQuote, I, DB, DQ> {
        Ok(BaseOrQuote::zero())
    }
}
