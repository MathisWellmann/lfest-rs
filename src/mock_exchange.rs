//! Convenince function used in tests.

use fpdec::{Dec, Decimal};

use crate::{account_tracker::NoAccountTracker, prelude::*};

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
        QuantityFilter {
            min_quantity: base!(0),
            max_quantity: base!(0),
            step_size: base!(0.01),
        },
        fee!(0.0002),
        fee!(0.0006),
    )
    .expect("works");
    let config = Config::new(quote!(1000), 200, contract_spec).unwrap();
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
        fee!(0.0002),
        fee!(0.0006),
    )
    .expect("works");
    let config = Config::new(starting_balance, 200, contract_spec).unwrap();
    Exchange::new(acc_tracker, config)
}
