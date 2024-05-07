use fpdec::{Dec, Decimal};
use getset::{CopyGetters, Getters, Setters};

use crate::{
    fee, leverage,
    prelude::{Currency, PriceFilter, QuantityFilter},
    types::{Error, Fee, Leverage},
    Result,
};

/// Specifies the details of the futures contract
#[derive(Debug, Clone, Getters, CopyGetters, Setters)]
pub struct ContractSpecification<Q>
where
    Q: Currency,
{
    /// Identifying ticker symbol
    #[getset(get = "pub", set = "pub")]
    ticker: String,

    /// The initial deposit required to open a new futures position.
    /// Expressed as a fraction.
    /// Eg. 1% (0.01) initial margin requirement, which is equal to 100x leverage.
    #[getset(get_copy = "pub")]
    init_margin_req: Decimal,

    /// The minimum amount that must be maintained in the traders account to
    /// keep existing positions open.
    /// Expressed as a fraction.
    /// Eg. 0.5% (0.005).
    #[getset(get_copy = "pub")]
    maintenance_margin: Decimal,

    /// The method for computing `mark-to-market`.
    #[getset(get_copy = "pub", set = "pub")]
    mark_method: MarkMethod,

    /// Pricing rules
    #[getset(get = "pub")]
    price_filter: PriceFilter,

    /// Quantity rules
    #[getset(get = "pub")]
    quantity_filter: QuantityFilter<Q>,

    /// The maker fee as a fraction. e.g.: 2.5 basis points rebate -> -0.00025
    #[getset(get_copy = "pub")]
    fee_maker: Fee,

    /// The taker fee as a fraction. e.g.: 10 basis points -> 0.0010
    #[getset(get_copy = "pub")]
    fee_taker: Fee,
}

impl<Q> ContractSpecification<Q>
where
    Q: Currency,
{
    /// Create a new `ContractSpecification` from the most basic parameters.
    ///
    /// # Arguments:
    /// `leverage`: The leverage dictates the margin requirements of a position.
    /// When a trader sets a user-defined leverage setting, they're essentially adjusting the margin requirements for their account.
    /// higher leverage setting means lower margin requirements, while a lower leverage setting means higher margin requirements.
    /// `maintenance_margin_fraction`: The fraction (in range [0..1]) that the maintenance margin will be relative to the computed `initial_margin`.
    /// `price_filter`: The rules for prices in the market
    /// `quantity_filter`: The rules for quantities in the market.
    /// `fee_maker`: The fee a maker pays.
    /// `fee_taker`: The fee a taker pays.
    pub fn new(
        leverage: Leverage,
        maintenance_margin_fraction: Decimal,
        price_filter: PriceFilter,
        quantity_filter: QuantityFilter<Q>,
        fee_maker: Fee,
        fee_taker: Fee,
    ) -> Result<Self> {
        if maintenance_margin_fraction > Dec!(1) || maintenance_margin_fraction <= Dec!(0) {
            return Err(Error::InvalidMaintenanceMarginFraction);
        }

        let initial_margin = Dec!(1) / *leverage.as_ref();

        Ok(Self {
            ticker: String::new(),
            init_margin_req: initial_margin,
            maintenance_margin: initial_margin * maintenance_margin_fraction,
            mark_method: MarkMethod::default(),
            price_filter,
            quantity_filter,
            fee_maker,
            fee_taker,
        })
    }
}

impl<Q> Default for ContractSpecification<Q>
where
    Q: Currency,
{
    fn default() -> Self {
        Self::new(
            leverage!(1),
            Dec!(0.5),
            PriceFilter::default(),
            QuantityFilter::default(),
            fee!(0.0002),
            fee!(0.0006),
        )
        .expect("Is valid")
    }
}

// TODO: actually switch between the methods.
/// Which price to use in `mark-to-market` calculations
#[derive(Debug, Clone, Copy)]
pub enum MarkMethod {
    /// Take the last mid price of the market.
    MidPrice,
    /// Use the best bid and ask to mark the position to market.
    BidAsk,
    /// Use Fair Price Marking to avoid unnecessary liquidations in highly leveraged products.
    /// Without this system, unnecessary liquidations may occur if the market is being manipulated,
    /// is illiquid, or the Mark Price swings unnecessarily relative to its Index Price.
    /// The system is able to achieve this by setting the Mark Price of the contract to the `FairPrice` instead of the `LastPrice`.
    FairPrice,
}

impl Default for MarkMethod {
    fn default() -> Self {
        Self::BidAsk
    }
}
