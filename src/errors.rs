#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
/// Defines the possible order errors that can occur when submitting a new order
pub enum OrderError {
    /// Maximum number of active orders reached
    MaxActiveOrders,
    /// Invalid limit price of order
    InvalidLimitPrice,
    /// Invalid trigger price for order. e.g.: sell stop market order trigger price > ask
    InvalidTriggerPrice,
    /// Invalid order size
    InvalidOrderSize,
    /// The account does not have enough available balance to submit the order
    NotEnoughAvailableBalance,
}
