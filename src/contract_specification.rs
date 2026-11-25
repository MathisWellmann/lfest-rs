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
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    EXPECT_CONVERSION,
    EXPECT_DECIMAL,
    leverage,
    prelude::{
        ConfigError,
        Currency,
        MaintenanceMarginSchedule,
        Maker,
        MarginCurrency,
        Mon,
        NotionalFilter,
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
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "I: Serialize, BaseOrQuote: Serialize, BaseOrQuote::PairedCurrency: Serialize",
        deserialize = "I: Deserialize<'de>, BaseOrQuote: Deserialize<'de>, BaseOrQuote::PairedCurrency: Deserialize<'de>"
    ))
)]
pub struct ContractSpecification<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
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

    /// Tiered maintenance margin schedule (defaults to a single flat tier at `maintenance_margin`).
    #[getset(get = "pub")]
    maintenance_margin_schedule: MaintenanceMarginSchedule<I, D, BaseOrQuote::PairedCurrency>,

    /// The method for computing `mark-to-market`.
    #[getset(get_copy = "pub", set = "pub")]
    mark_method: MarkMethod,

    /// Pricing rules
    #[getset(get = "pub")]
    price_filter: PriceFilter<I, D>,

    /// Quantity rules
    #[getset(get = "pub")]
    quantity_filter: QuantityFilter<I, D, BaseOrQuote>,

    /// Optional order notional rules (measured in margin currency).
    #[getset(get = "pub", set = "pub")]
    notional_filter: NotionalFilter<I, D, BaseOrQuote::PairedCurrency>,

    /// The maker fee rate (negative values represent maker rebates).
    #[getset(get_copy = "pub")]
    fee_maker: Fee<I, D, Maker>,

    /// The taker fee rate (must be non-negative).
    #[getset(get_copy = "pub")]
    fee_taker: Fee<I, D, Taker>,

    /// Optional extra clearance fee rate charged on forced liquidations in addition to `fee_taker`.
    /// Defaults to zero for backwards compatibility.
    #[getset(get_copy = "pub")]
    liquidation_fee: Decimal<I, D>,

    /// Optional default [`crate::stop_orders::StopProtection`] collar applied to stop-market
    /// orders on this contract unless overridden on the individual stop order.
    #[getset(get_copy = "pub", set = "pub")]
    stop_protection: Option<crate::stop_orders::StopProtection<I, D>>,

    /// Market-impact / slippage model applied to aggressive market order fills
    /// (defaults to [`crate::slippage::SlippageModel::ZeroImpact`]).
    #[getset(get_copy = "pub", set = "pub")]
    slippage_model: crate::slippage::SlippageModel<I, D>,
}

impl<I, const D: u8, BaseOrQuote> ContractSpecification<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
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
    /// `fee_maker`: The fee a maker pays (negative values are maker rebates).
    /// `fee_taker`: The fee a taker pays (must be non-negative).
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
        if *fee_taker.as_ref() < Decimal::zero() {
            return Err(ConfigError::InvalidTakerFee);
        }

        let init_margin_req = leverage.init_margin_req();
        let maint_margin_rate = init_margin_req * maintenance_margin;
        let maintenance_margin_schedule = MaintenanceMarginSchedule::flat(maint_margin_rate)?;

        Ok(Self {
            ticker: String::new(),
            init_margin_req,
            maintenance_margin: maint_margin_rate,
            maintenance_margin_schedule,
            mark_method: MarkMethod::default(),
            price_filter,
            quantity_filter,
            notional_filter: NotionalFilter::default(),
            fee_maker,
            fee_taker,
            liquidation_fee: Decimal::zero(),
            stop_protection: None,
            slippage_model: crate::slippage::SlippageModel::ZeroImpact,
        })
    }

    /// Configure a [`crate::slippage::SlippageModel`] for aggressive market fills.
    #[must_use]
    pub fn with_slippage_model(
        mut self,
        slippage_model: crate::slippage::SlippageModel<I, D>,
    ) -> Self {
        self.slippage_model = slippage_model;
        self
    }

    /// Attach a default [`crate::stop_orders::StopProtection`] collar to this contract.
    #[must_use]
    pub fn with_stop_protection(
        mut self,
        protection: crate::stop_orders::StopProtection<I, D>,
    ) -> Self {
        self.stop_protection = Some(protection);
        self
    }

    /// Replace the flat maintenance margin schedule with a tiered bracket schedule.
    ///
    /// `maintenance_margin()` then reports the tier-0 rate; risk checks use the full schedule.
    ///
    /// # Errors
    /// Returns [`ConfigError::InvalidMaintenanceMarginTier`] if any tier's maintenance rate is
    /// `>=` the initial margin rate, because a position of that size would be liquidatable
    /// right at open.
    pub fn with_maintenance_margin_schedule(
        mut self,
        schedule: MaintenanceMarginSchedule<I, D, BaseOrQuote::PairedCurrency>,
    ) -> Result<Self, ConfigError> {
        // Rates are non-decreasing (schedule invariant), so the last tier is the maximum.
        if schedule.max_maintenance_margin_rate() >= self.init_margin_req {
            return Err(ConfigError::InvalidMaintenanceMarginTier);
        }
        self.maintenance_margin = schedule.tiers()[0].maintenance_margin_rate();
        assert!(self.maintenance_margin < self.init_margin_req);
        self.maintenance_margin_schedule = schedule;
        Ok(self)
    }

    /// Configure an optional extra liquidation clearance fee rate (in `[0, 1)`),
    /// charged in addition to `fee_taker` during forced liquidations.
    ///
    /// # Errors
    /// Returns [`ConfigError::InvalidLiquidationFee`] if `fee < 0` or `fee >= 1`.
    pub fn with_liquidation_fee(mut self, fee: Decimal<I, D>) -> Result<Self, ConfigError> {
        if fee < Decimal::zero() || fee >= Decimal::one() {
            return Err(ConfigError::InvalidLiquidationFee);
        }
        self.liquidation_fee = fee;
        Ok(self)
    }

    /// Attach a [`NotionalFilter`] to this contract specification.
    #[must_use]
    pub fn with_notional_filter(
        mut self,
        notional_filter: NotionalFilter<I, D, BaseOrQuote::PairedCurrency>,
    ) -> Self {
        self.notional_filter = notional_filter;
        self
    }

    /// Configure the [`MarkMethod`] used for mark-to-market pricing.
    #[must_use]
    pub fn with_mark_method(mut self, mark_method: MarkMethod) -> Self {
        self.mark_method = mark_method;
        self
    }

    /// Configure the identifying ticker symbol.
    #[must_use]
    pub fn with_ticker(mut self, ticker: impl Into<String>) -> Self {
        self.ticker = ticker.into();
        self
    }
}

impl<I, const D: u8, BaseOrQuote> Default for ContractSpecification<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    fn default() -> Self {
        Self::new(
            leverage!(1),
            Decimal::ONE / Decimal::TWO,
            PriceFilter::default(),
            QuantityFilter::default(),
            Fee::from(
                Decimal::try_from_scaled(I::from(2).expect(EXPECT_CONVERSION), 4)
                    .expect(EXPECT_DECIMAL),
            ),
            Fee::from(
                Decimal::try_from_scaled(I::from(6).expect(EXPECT_CONVERSION), 4)
                    .expect(EXPECT_DECIMAL),
            ),
        )
        .expect("Is valid")
    }
}

/// Which price to use in `mark-to-market` calculations
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MarkMethod {
    /// Take the last mid price of the market.
    MidPrice,
    /// Use the best bid and ask to mark the position to market.
    #[default]
    BidAsk,
    /// Use Fair Price Marking to avoid unnecessary liquidations in highly leveraged products.
    /// Without this system, unnecessary liquidations may occur if the market is being manipulated,
    /// is illiquid, or the Mark Price swings unnecessarily relative to its Index Price.
    /// The system is able to achieve this by setting the Mark Price of the contract to the `FairPrice` instead of the `LastPrice`.
    FairPrice,
}
