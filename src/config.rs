use crate::{Error, FuturesTypes, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Define the Exchange configuration
pub struct Config {
    /// The maker fee as a fraction. e.g.: 2.5 basis points rebate -> -0.00025
    fee_maker: f64,
    /// The taker fee as a fraction. e.g.: 10 basis points -> 0.0010
    fee_taker: f64,
    /// The starting balance of account
    starting_balance: f64,
    /// The leverage used for the position
    leverage: f64,
    /// The type of futures to simulate
    futures_type: FuturesTypes,
    /// To identify an exchange by a code
    identification: String,
    /// Sets the order timestamps on submit_order() call, if enabled
    set_order_timestamps: bool,
}

impl Config {
    /// Create a new Config
    /// # Arguments
    /// fee_maker: The maker fee as fraction, e.g 6bp -> 0.0006
    /// fee_taker: The taker fee as fraction
    /// starting_balance: Initial Wallet Balance, denoted in QUOTE if using
    /// linear futures, denoted in BASE for inverse futures leverage: The
    /// positions leverage futures_type: The type of futures contract to
    /// simulate identification: A way to identify an exchange
    /// # Returns
    /// Either a valid Config or an Error
    #[inline]
    pub fn new(
        fee_maker: f64,
        fee_taker: f64,
        starting_balance: f64,
        leverage: f64,
        futures_type: FuturesTypes,
        identification: String,
        set_order_timestamps: bool,
    ) -> Result<Config> {
        if leverage <= 0.0 {
            return Err(Error::ConfigWrongLeverage);
        }
        if starting_balance <= 0.0 {
            return Err(Error::ConfigWrongStartingBalance);
        }
        Ok(Config {
            fee_maker,
            fee_taker,
            starting_balance,
            leverage,
            futures_type,
            identification,
            set_order_timestamps,
        })
    }

    /// Return the maker fee of this config
    #[inline(always)]
    pub fn fee_maker(&self) -> f64 {
        self.fee_maker
    }

    /// Return the taker fee of this config
    #[inline(always)]
    pub fn fee_taker(&self) -> f64 {
        self.fee_taker
    }

    /// Return the starting wallet balance of this Config
    #[inline(always)]
    pub fn starting_balance(&self) -> f64 {
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
}
