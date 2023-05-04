use fpdec::Decimal;

use crate::prelude::{Currency, PriceFilter, QuantityFilter};

#[derive(Debug, Clone)]
pub struct ContractSpecification<S>
where
    S: Currency,
{
    ticker: String,
    initial_margin: Decimal,
    maintenance_margin: Decimal,
    mark_method: MarkMethod,
    price_filter: PriceFilter,
    quantity_filter: QuantityFilter<S>,
}

#[derive(Debug, Clone)]
pub enum MarkMethod {
    LastPrice,
    FairPrice,
}
