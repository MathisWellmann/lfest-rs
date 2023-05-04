use fpdec::Decimal;

use crate::prelude::{Currency, PriceFilter, QuantityFilter};

pub struct ContractSpecification<S>
where
    S: Currency,
{
    ticker: String,
    initial_margin: Decimal,
    maintenance_margin: Decimal,
    mark_method: MarkMethod,
    contract_size: S,
    price_filter: PriceFilter,
    quantity_filter: QuantityFilter<S>,
}

pub enum MarkMethod {
    LastPrice,
    FairPrice,
}
