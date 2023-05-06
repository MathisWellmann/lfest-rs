use fpdec::Decimal;

use crate::{
    prelude::{Currency, PriceFilter, QuantityFilter},
    types::Fee,
};

/// Specifies the details of the futures contract
#[derive(Debug, Clone)]
pub struct ContractSpecification<S>
where
    S: Currency,
{
    /// Identifying ticker symbol
    pub ticker: String,
    /// The initial margin as a fraction.
    pub initial_margin: Decimal,
    /// The required maintenance margin as a fraction.
    pub maintenance_margin: Decimal,
    /// The methods for computing `mark-to-market`
    pub mark_method: MarkMethod,
    /// Pricing rules
    pub price_filter: PriceFilter,
    /// Quantity rules
    pub quantity_filter: QuantityFilter<S>,
    /// The maker fee as a fraction. e.g.: 2.5 basis points rebate -> -0.00025
    pub fee_maker: Fee,
    /// The taker fee as a fraction. e.g.: 10 basis points -> 0.0010
    pub fee_taker: Fee,
}

/// Which price to use in `mark-to-market` calculations
#[derive(Debug, Clone)]
pub enum MarkMethod {
    /// Take the last mid price of the market.
    MidPrice,
    /// Use Fair Price Marking to avoid unnecessary liquidations in highly leveraged products.
    /// Without this system, unnecessary liquidations may occur if the market is being manipulated,
    /// is illiquid, or the Mark Price swings unnecessarily relative to its Index Price.
    /// The system is able to achieve this by setting the Mark Price of the contract to the `FairPrice` instead of the `LastPrice`.
    FairPrice,
}
