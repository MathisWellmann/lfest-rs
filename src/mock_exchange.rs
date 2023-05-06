use fpdec::{Dec, Decimal};

use crate::prelude::*;

/// Constructs a mock exchange for testing.
pub fn mock_exchange() -> Exchange<NoAccountTracker, BaseCurrency> {
    let acc_tracker = NoAccountTracker::default();
    let contract_specification = ContractSpecification {
        ticker: "TESTUSD".to_string(),
        initial_margin: Dec!(0.01),
        maintenance_margin: Dec!(0.02),
        mark_method: MarkMethod::MidPrice,
        price_filter: PriceFilter::default(),
        quantity_filter: QuantityFilter::default(),
    };
    let config = Config::new(
        fee!(0.001),
        fee!(0.001),
        quote!(1000),
        200,
        contract_specification,
    )
    .unwrap();
    Exchange::new(acc_tracker, config)
}
