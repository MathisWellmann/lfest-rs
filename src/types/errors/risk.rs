use thiserror::Error;

/// The error that the `RiskEngine` outputs, if any.
#[derive(Error, Debug, Clone, Eq, PartialEq)]
#[allow(missing_docs, reason = "Self documenting")]
pub enum RiskError {
    #[error(transparent)]
    NotEnoughAvailableBalance(#[from] NotEnoughAvailableBalance),

    #[error("The position will be liquidated!")]
    Liquidate,
}

#[derive(Error, Debug, Clone, Eq, PartialEq, derive_more::Display)]
#[allow(missing_docs, reason = "Self documenting")]
pub struct NotEnoughAvailableBalance;
