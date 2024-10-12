use getset::{CopyGetters, Getters, Setters};

use crate::{
    leverage,
    prelude::{ConfigError, CurrencyMarker, Maker, Mon, PriceFilter, QuantityFilter, Taker},
    types::{Fee, Leverage},
};

/// Specifies the details of the futures contract
#[derive(Debug, Clone, Getters, CopyGetters, Setters)]
pub struct ContractSpecification<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    /// Identifying ticker symbol
    #[getset(get = "pub", set = "pub")]
    ticker: String,

    /// The initial deposit required to open a new futures position.
    /// Expressed as a fraction.
    /// Eg. 1% (0.01) initial margin requirement, which is equal to 100x leverage.
    #[getset(get_copy = "pub")]
    init_margin_req: T,

    /// The minimum amount that must be maintained in the traders account to
    /// keep existing positions open.
    /// Expressed as a fraction.
    /// Eg. 0.5% (0.005).
    #[getset(get_copy = "pub")]
    maintenance_margin: T,

    /// The method for computing `mark-to-market`.
    #[getset(get_copy = "pub", set = "pub")]
    mark_method: MarkMethod,

    /// Pricing rules
    #[getset(get = "pub")]
    price_filter: PriceFilter<T>,

    /// Quantity rules
    #[getset(get = "pub")]
    quantity_filter: QuantityFilter<T, BaseOrQuote>,

    /// The maker fee as parts per 100_000
    #[getset(get_copy = "pub")]
    fee_maker: Fee<Maker>,

    /// The taker fee as parts per 100_000
    #[getset(get_copy = "pub")]
    fee_taker: Fee<Taker>,
}

impl<T, BaseOrQuote> ContractSpecification<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
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
        maintenance_margin_fraction: T,
        price_filter: PriceFilter<T>,
        quantity_filter: QuantityFilter<T, BaseOrQuote>,
        fee_maker: Fee<Maker>,
        fee_taker: Fee<Taker>,
    ) -> Result<Self, ConfigError> {
        if maintenance_margin_fraction > T::one() || maintenance_margin_fraction <= T::zero() {
            return Err(ConfigError::InvalidMaintenanceMarginFraction);
        }

        let init_margin_req = leverage.init_margin_req();

        Ok(Self {
            ticker: String::new(),
            init_margin_req,
            maintenance_margin: init_margin_req * maintenance_margin_fraction,
            mark_method: MarkMethod::default(),
            price_filter,
            quantity_filter,
            fee_maker,
            fee_taker,
        })
    }
}

impl<T, BaseOrQuote> Default for ContractSpecification<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: CurrencyMarker<T>,
{
    fn default() -> Self {
        Self::new(
            leverage!(1),
            T::one() / T::from(2),
            PriceFilter::default(),
            QuantityFilter::default(),
            Fee::from_basis_points(2),
            Fee::from_basis_points(6),
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
