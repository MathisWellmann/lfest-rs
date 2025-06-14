//! Convenience function used in tests.

use std::num::NonZeroUsize;

use const_decimal::Decimal;

use crate::{prelude::*, utils::NoUserOrderId};

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
/// and the margin currency is `QuoteCurrency`
pub fn mock_exchange_linear() -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId>
{
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Decimal::try_from_scaled(5, 1).unwrap(),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).unwrap(),
        test_fee_maker(),
        test_fee_taker(),
    )
    .expect("works");
    let config = Config::new(
        QuoteCurrency::new(1000, 0),
        NonZeroUsize::new(10).unwrap(),
        contract_spec,
        OrderRateLimits::default(),
    )
    .unwrap();
    Exchange::new(config)
}

/// Constructs a mock exchange (for linear futures) for testing.
/// The size is denoted in `BaseCurrency`
/// and the margin currency is `QuoteCurency`
pub fn mock_exchange_linear_with_account_tracker(
    starting_balance: QuoteCurrency<i64, DECIMALS>,
) -> Exchange<i64, DECIMALS, BaseCurrency<i64, DECIMALS>, NoUserOrderId> {
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Decimal::try_from_scaled(5, 1).unwrap(),
        PriceFilter::default(),
        QuantityFilter::new(None, None, BaseCurrency::new(1, 2)).unwrap(),
        test_fee_maker(),
        test_fee_taker(),
    )
    .expect("works");
    let config = Config::new(
        starting_balance,
        NonZeroUsize::new(200).unwrap(),
        contract_spec,
        OrderRateLimits::default(),
    )
    .unwrap();
    Exchange::new(config)
}

/// Constructs a mock exchange (for inverse futures) for testing.
pub fn mock_exchange_inverse(
    starting_balance: BaseCurrency<i64, DECIMALS>,
) -> Exchange<i64, DECIMALS, QuoteCurrency<i64, DECIMALS>, NoUserOrderId> {
    let contract_spec = ContractSpecification::new(
        leverage!(1),
        Decimal::try_from_scaled(5, 1).expect("works"),
        PriceFilter::default(),
        QuantityFilter::default(),
        test_fee_maker(),
        test_fee_taker(),
    )
    .expect("works");
    let config = Config::new(
        starting_balance,
        NonZeroUsize::new(200).unwrap(),
        contract_spec,
        OrderRateLimits::default(),
    )
    .unwrap();
    Exchange::new(config)
}
