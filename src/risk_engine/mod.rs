//! A risk engine is an essential component of a futures exchange
//! that helps to manage and mitigate risks associated with trading futures contracts.
//! The risk engine is responsible for handling a range of functions related to risk management, including the following:
//!
//! 1. Margin Requirements:
//!     The risk engine calculates and monitors margin requirements for each futures contract.
//!     Margin is a deposit that traders are required to maintain to cover potential losses in case the price of the underlying asset moves against their position.
//!     The risk engine calculates the initial margin required to enter into a position
//!     and then monitors the margin requirements on a real-time basis to ensure that they are met.
//!
//! 2. Position Limits:
//!     The risk engine enforces position limits on each futures contract to prevent excessive speculation and manipulation of prices.
//!     Position limits are set by the exchange and restrict the maximum number of contracts that any trader can hold for a particular futures contract.

mod isolated_margin;
mod risk_engine_trait;

pub(crate) use isolated_margin::IsolatedMarginRiskEngine;
pub(crate) use risk_engine_trait::RiskEngine;
