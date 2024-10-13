use const_decimal::Decimal;
use getset::{CopyGetters, Getters, Setters};
use num_traits::One;

use crate::{
    leverage,
    prelude::{
        BasisPointFrac, ConfigError, CurrencyMarker, Maker, Mon, PriceFilter, QuantityFilter, Taker,
    },
    types::{Fee, Leverage},
};

/// Specifies the details of the futures contract
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `DB`: The constant decimal precision of the `BaseCurrency`.
/// - `DQ`: The constant decimal precision of the `QuoteCurrency`.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
#[derive(Debug, Clone, Getters, CopyGetters, Setters)]
pub struct ContractSpecification<I, const DB: u8, const DQ: u8, BaseOrQuote>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
{
    /// Identifying ticker symbol
    #[getset(get = "pub", set = "pub")]
    ticker: String,

    /// The initial deposit required to open a new futures position.
    /// Expressed as basis points.
    #[getset(get_copy = "pub")]
    init_margin_req: BasisPointFrac,

    /// The minimum amount that must be maintained in the traders account to
    /// keep existing positions open.
    /// Expressed as basis points.
    #[getset(get_copy = "pub")]
    maintenance_margin: BasisPointFrac,

    /// The method for computing `mark-to-market`.
    #[getset(get_copy = "pub", set = "pub")]
    mark_method: MarkMethod,

    /// Pricing rules
    #[getset(get = "pub")]
    price_filter: PriceFilter<I, DB, DQ>,

    /// Quantity rules
    #[getset(get = "pub")]
    quantity_filter: QuantityFilter<I, DB, DQ, BaseOrQuote>,

    /// The maker fee as parts per 100_000
    #[getset(get_copy = "pub")]
    fee_maker: Fee<Maker>,

    /// The taker fee as parts per 100_000
    #[getset(get_copy = "pub")]
    fee_taker: Fee<Taker>,
}

impl<I, const DB: u8, const DQ: u8, BaseOrQuote> ContractSpecification<I, DB, DQ, BaseOrQuote>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
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
        maintenance_margin: BasisPointFrac,
        price_filter: PriceFilter<I, DB, DQ>,
        quantity_filter: QuantityFilter<I, DB, DQ, BaseOrQuote>,
        fee_maker: Fee<Maker>,
        fee_taker: Fee<Taker>,
    ) -> Result<Self, ConfigError> {
        if maintenance_margin > BasisPointFrac::from(Decimal::one())
            || maintenance_margin <= BasisPointFrac::from(Decimal::one())
        {
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

impl<I, const DB: u8, const DQ: u8, BaseOrQuote> Default
    for ContractSpecification<I, DB, DQ, BaseOrQuote>
where
    I: Mon<DB> + Mon<DQ>,
    BaseOrQuote: CurrencyMarker<I, DB, DQ>,
{
    fn default() -> Self {
        Self::new(
            leverage!(1),
            BasisPointFrac::from(Decimal::try_from_scaled(5, 1).unwrap()),
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
