/// The solvency of the account after a fill was settled and the collateral reconciled.
///
/// A position-reducing fill is never rejected by the venue, so settling it can leave the
/// account with less equity than its required collateral. The exchange then reconciles
/// the account like a real venue would and reports the outcome through this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Solvency {
    /// The account equity covers the required collateral.
    Solvent,
    /// The equity does not cover the position's initial margin requirement even after
    /// force-cancelling every resting order. No new risk-increasing orders will be
    /// admitted (the available balance is zero), but the position is kept as long as
    /// it satisfies the maintenance margin.
    InitialMarginDeficit,
    /// The equity fell below the position's maintenance margin requirement;
    /// the resting orders were cancelled and the position was force-closed.
    Liquidated,
    /// Realized losses exhausted the account equity entirely; the excess loss is
    /// absorbed by the venue and recorded as `Balances::bad_debt`.
    Bankrupt,
}
