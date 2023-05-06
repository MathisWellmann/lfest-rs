//! Convenince function used in tests.

use fpdec::{Dec, Decimal};

use crate::{account_tracker::NoAccountTracker, prelude::*};

/// Constructs a mock exchange for testing.
/// The size is denoted in `BaseCurrency`
/// and the margin currency is `QuoteCurency`
pub fn mock_exchange_base() -> Exchange<NoAccountTracker, BaseCurrency> {
    let acc_tracker = NoAccountTracker::default();
    let contract_specification = ContractSpecification {
        ticker: "TESTUSD".to_string(),
        initial_margin: Dec!(0.01),
        maintenance_margin: Dec!(0.02),
        mark_method: MarkMethod::MidPrice,
        price_filter: PriceFilter::default(),
        quantity_filter: QuantityFilter::default(),
        fee_maker: fee!(0.0002),
        fee_taker: fee!(0.0006),
    };
    let config = Config::new(quote!(1000), 200, leverage!(1), contract_specification).unwrap();
    Exchange::new(acc_tracker, config)
}

/// Constructs a mock exchange for testing.
pub fn mock_exchange_quote() -> Exchange<NoAccountTracker, QuoteCurrency> {
    let acc_tracker = NoAccountTracker::default();
    let contract_specification = ContractSpecification {
        ticker: "TESTUSD".to_string(),
        initial_margin: Dec!(0.01),
        maintenance_margin: Dec!(0.02),
        mark_method: MarkMethod::MidPrice,
        price_filter: PriceFilter::default(),
        quantity_filter: QuantityFilter::default(),
        fee_maker: fee!(0.0002),
        fee_taker: fee!(0.0006),
    };
    let config = Config::new(base!(10), 200, leverage!(1), contract_specification).unwrap();
    Exchange::new(acc_tracker, config)
}
