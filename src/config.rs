use crate::{Currency, Error, Fee, FuturesTypes, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Define the Exchange configuration
pub struct Config<M> {
    /// The maker fee as a fraction. e.g.: 2.5 basis points rebate -> -0.00025
    fee_maker: Fee,
    /// The taker fee as a fraction. e.g.: 10 basis points -> 0.0010
    fee_taker: Fee,
    /// The starting balance of account
    starting_balance: M,
    /// The leverage used for the position
    leverage: f64,
    /// The type of futures to simulate
    futures_type: FuturesTypes,
    /// To identify an exchange by a code
    identification: String,
    /// Sets the order timestamps on submit_order() call, if enabled
    set_order_timestamps: bool,
    /// The maximum number of open orders the user can have at any given time
    max_num_open_orders: usize,
}

impl<M> Config<M>
where M: Currency
{
    /// Create a new Config.
    ///
    /// # Arguments:
    /// `fee_maker`: The maker fee as fraction, e.g 6bp -> 0.0006
    /// `fee_taker`: The taker fee as fraction
    /// `starting_balance`: Initial Wallet Balance, denoted in QUOTE if using
    /// linear futures, denoted in BASE for inverse futures
    /// `leverage`: The positions leverage.
    /// `futures_type`: The type of futures contract to
    /// simulate.
    /// `identification`: A way to identify an exchange
    /// `set_order_timestamps`: Whether the exchange should set order
    /// timestamps.
    /// `max_num_open_orders`: The maximum number of open ordes a user can have
    /// at any time.
    ///
    /// # Returns:
    /// Either a valid Config or an Error
    #[inline]
    pub fn new(
        fee_maker: Fee,
        fee_taker: Fee,
        starting_balance: M,
        leverage: f64,
        futures_type: FuturesTypes,
        identification: String,
        set_order_timestamps: bool,
        max_num_open_orders: usize,
    ) -> Result<Self> {
        if leverage < 1.0 {
            return Err(Error::ConfigWrongLeverage);
        }
        if max_num_open_orders == 0 {
            return Err(Error::InvalidMaxNumOpenOrders);
        }
        Ok(Config {
            fee_maker,
            fee_taker,
            starting_balance,
            leverage,
            futures_type,
            identification,
            set_order_timestamps,
            max_num_open_orders,
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
    pub fn leverage(&self) -> f64 {
        self.leverage
    }

    /// Return the FuturesType of the Config
    #[inline(always)]
    pub fn futures_type(&self) -> FuturesTypes {
        self.futures_type
    }

    /// Return the exchange identification
    #[inline(always)]
    pub fn identification(&self) -> &str {
        &self.identification
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
}
