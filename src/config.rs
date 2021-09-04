use crate::FuturesTypes;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// Define the Exchange configuration
pub struct Config {
    /// The maker fee as a fraction. e.g.: 2.5 basis points rebate -> -0.00025
    pub fee_maker: f64,
    /// The taker fee as a fraction. e.g.: 10 basis points -> 0.0010
    pub fee_taker: f64,
    /// The starting balance of account
    pub starting_balance: f64,
    /// The leverage used for the position
    pub leverage: f64,
    /// The type of futures to simulate
    pub futures_type: FuturesTypes,
}
