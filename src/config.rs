use crate::{
    errors::{Error, Result},
    prelude::{PriceFilter, QuantityFilter},
    types::{Currency, Fee, Leverage},
};

#[derive(Debug, Clone)]
/// Define the Exchange configuration
pub struct Config<M>
where
    M: Currency,
{
    /// The maker fee as a fraction. e.g.: 2.5 basis points rebate -> -0.00025
    fee_maker: Fee,
    /// The taker fee as a fraction. e.g.: 10 basis points -> 0.0010
    fee_taker: Fee,
    /// The starting balance of account (denoted in margin currency).
    /// The concrete `Currency` here defines the futures type.
    /// If `QuoteCurrency` is used as the margin currency,
    /// then its a linear futures contract.
    /// If `BaseCurrency` is used as the margin currency,
    /// then its an inverse futures contract.
    starting_balance: M,
    /// The leverage used for the position
    leverage: Leverage,
    /// Sets the order timestamps on submit_order() call, if enabled
    set_order_timestamps: bool,
    /// The maximum number of open orders the user can have at any given time
    max_num_open_orders: usize,
    /// Filters for limit order pricing
    price_filter: PriceFilter,
    /// Filters for order quantity
    quantity_filter: QuantityFilter<M::PairedCurrency>,
}

impl<M> Config<M>
where
    M: Currency,
{
    /// Create a new Config.
    ///
    /// # Arguments:
    /// `fee_maker`: The maker fee as fraction, e.g 6bp -> 0.0006
    /// `fee_taker`: The taker fee as fraction
    /// `starting_balance`: Initial Wallet Balance, denoted in QUOTE if using
    /// linear futures, denoted in BASE for inverse futures
    /// `leverage`: The positions leverage.
    /// `set_order_timestamps`: Whether the exchange should set order
    /// timestamps.
    /// `max_num_open_orders`: The maximum number of open ordes a user can have
    /// at any time.
    /// `price_filter`: Filters for limit order pricing
    /// `quantity_filter`: Filters for order quantities
    ///
    /// # Returns:
    /// Either a valid Config or an Error
    #[allow(clippy::complexity)]
    pub fn new(
        fee_maker: Fee,
        fee_taker: Fee,
        starting_balance: M,
        leverage: Leverage,
        set_order_timestamps: bool,
        max_num_open_orders: usize,
        price_filter: PriceFilter,
        quantity_filter: QuantityFilter<M::PairedCurrency>,
    ) -> Result<Self> {
        if max_num_open_orders == 0 {
            return Err(Error::InvalidMaxNumOpenOrders);
        }
        if starting_balance <= M::new_zero() {
            return Err(Error::InvalidStartingBalance);
        }
        Ok(Config {
            fee_maker,
            fee_taker,
            starting_balance,
            leverage,
            set_order_timestamps,
            max_num_open_orders,
            price_filter,
            quantity_filter,
        })
    }

    /// Return the maker fee of this config
    #[inline(always)]
    pub fn fee_maker(&self) -> Fee {
        self.fee_maker
    }

    /// Return the taker fee of this config
    #[inline(always)]
    pub fn fee_taker(&self) -> Fee {
        self.fee_taker
    }

    /// Return the starting wallet balance of this Config
    #[inline(always)]
    pub fn starting_balance(&self) -> M {
        self.starting_balance
    }

    /// Return the leverage of the Config
    #[inline(always)]
    pub fn leverage(&self) -> Leverage {
        self.leverage
    }

    /// Return whether or not the Exchange is configured to set order timestamps
    /// in submit_order method
    #[inline(always)]
    pub fn set_order_timestamps(&self) -> bool {
        self.set_order_timestamps
    }

    /// Returns the maximum number of open orders that are allowed
    #[inline(always)]
    pub fn max_num_open_orders(&self) -> usize {
        self.max_num_open_orders
    }

    /// Return a reference to the `PriceFilter`
    #[inline(always)]
    pub fn price_filter(&self) -> &PriceFilter {
        &self.price_filter
    }

    /// Return a reference to the `QuantityFilter`
    #[inline(always)]
    pub fn quantity_filter(&self) -> &QuantityFilter<M::PairedCurrency> {
        &self.quantity_filter
    }
}
