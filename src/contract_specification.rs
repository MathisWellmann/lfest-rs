use const_decimal::Decimal;
use getset::{
    CopyGetters,
    Getters,
    Setters,
};
use num_traits::{
    One,
    Zero,
};

use crate::{
    leverage,
    prelude::{
        ConfigError,
        Currency,
        Maker,
        Mon,
        PriceFilter,
        QuantityFilter,
        Taker,
    },
    types::{
        Fee,
        Leverage,
    },
};

/// Specifies the details of the futures contract
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currencies
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
#[derive(Debug, Clone, Getters, CopyGetters, Setters)]
pub struct ContractSpecification<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    /// Identifying ticker symbol
    #[getset(get = "pub", set = "pub")]
    ticker: String,

    /// The initial deposit required to open a new futures position.
    /// Expressed as basis points.
    #[getset(get_copy = "pub")]
    init_margin_req: Decimal<I, D>,

    /// The minimum amount that must be maintained in the traders account to
    /// keep existing positions open.
    /// Expressed as basis points.
    #[getset(get_copy = "pub")]
    maintenance_margin: Decimal<I, D>,

    /// The method for computing `mark-to-market`.
    #[getset(get_copy = "pub", set = "pub")]
    mark_method: MarkMethod,

    /// Pricing rules
    #[getset(get = "pub")]
    price_filter: PriceFilter<I, D>,

    /// Quantity rules
    #[getset(get = "pub")]
    quantity_filter: QuantityFilter<I, D, BaseOrQuote>,

    /// The maker fee as parts per 100_000
    #[getset(get_copy = "pub")]
    fee_maker: Fee<I, D, Maker>,

    /// The taker fee as parts per 100_000
    #[getset(get_copy = "pub")]
    fee_taker: Fee<I, D, Taker>,
}

impl<I, const D: u8, BaseOrQuote> ContractSpecification<I, D, BaseOrQuote>
where
    I: Mon<D> + Mon<D>,
    BaseOrQuote: Currency<I, D>,
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
        leverage: Leverage<I, D>,
        maintenance_margin: Decimal<I, D>,
        price_filter: PriceFilter<I, D>,
        quantity_filter: QuantityFilter<I, D, BaseOrQuote>,
        fee_maker: Fee<I, D, Maker>,
        fee_taker: Fee<I, D, Taker>,
    ) -> Result<Self, ConfigError> {
        if maintenance_margin > Decimal::one() || maintenance_margin <= Decimal::zero() {
            return Err(ConfigError::InvalidMaintenanceMarginFraction);
        }

        let init_margin_req = leverage.init_margin_req();

        Ok(Self {
            ticker: String::new(),
            init_margin_req,
            maintenance_margin: init_margin_req * maintenance_margin,
            mark_method: MarkMethod::default(),
            price_filter,
            quantity_filter,
            fee_maker,
            fee_taker,
        })
    }
}

impl<I, const D: u8, BaseOrQuote> Default for ContractSpecification<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    fn default() -> Self {
        Self::new(
            leverage!(1),
            Decimal::one() / Decimal::TWO,
            PriceFilter::default(),
            QuantityFilter::default(),
            Fee::from(Decimal::try_from_scaled(I::from(2).unwrap(), 4).unwrap()),
            Fee::from(Decimal::try_from_scaled(I::from(6).unwrap(), 4).unwrap()),
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
