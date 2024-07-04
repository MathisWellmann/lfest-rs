/// The error that the `RiskEngine` outputs, if any.
#[derive(thiserror::Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum RiskError {
    #[error("The `Trader` does not have enough balance.")]
    NotEnoughAvailableBalance,

    #[error("The position will be liquidated!")]
    Liquidate,
}
