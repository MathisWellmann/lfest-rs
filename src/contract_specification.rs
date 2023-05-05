use fpdec::Decimal;

use crate::prelude::{Currency, PriceFilter, QuantityFilter};

/// Specifies the details of the futures contract
#[derive(Debug, Clone)]
pub struct ContractSpecification<S>
where
    S: Currency,
{
    /// Identifying ticker symbol
    ticker: String,
    /// The initial margin as a fraction.
    initial_margin: Decimal,
    /// The required maintenance margin as a fraction.
    maintenance_margin: Decimal,
    /// The methods for computing `mark-to-market`
    mark_method: MarkMethod,
    /// Pricing rules
    price_filter: PriceFilter,
    /// Quantity rules
    quantity_filter: QuantityFilter<S>,
}

/// Which price to use in `mark-to-market` calculations
#[derive(Debug, Clone)]
pub enum MarkMethod {
    /// Take the last market price.
    LastPrice,
    /// Use Fair Price Marking to avoid unnecessary liquidations in highly leveraged products.
    /// Without this system, unnecessary liquidations may occur if the market is being manipulated,
    /// is illiquid, or the Mark Price swings unnecessarily relative to its Index Price.
    /// The system is able to achieve this by setting the Mark Price of the contract to the `FairPrice` instead of the `LastPrice`.
    FairPrice,
}
