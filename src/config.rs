use crate::FuturesTypes;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
}

impl Config {
    /// Create a new Config
    /// # Returns
    /// Either a valid Config or an Error
    #[must_use]
    #[inline]
    pub fn new(
        fee_maker: f64,
        fee_taker: f64,
        starting_balance: f64,
        leverage: f64,
        futures_type: FuturesTypes,
    ) -> Result<Config> {
        if leverage < 1.0 {
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
}
