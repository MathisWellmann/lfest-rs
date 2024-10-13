mod errors;
mod fee;
mod leverage;
mod limit_order;
mod market_order;
mod order_id;
mod order_meta;
mod order_status;
mod order_update;
mod re_pricing;
mod side;
mod smol_currency;
mod timestamp_ns;

use const_decimal::Decimal;
pub use errors::*;
pub use fee::{Fee, Maker, Taker};
pub use leverage::Leverage;
pub use limit_order::LimitOrder;
pub use market_order::MarketOrder;
pub use order_id::OrderId;
pub use order_meta::ExchangeOrderMeta;
pub use order_status::{Filled, FilledQuantity, NewOrder, Pending};
pub use order_update::LimitOrderUpdate;
pub use re_pricing::RePricing;
pub use side::Side;
pub use smol_currency::{BaseCurrency, CurrencyMarker, MarginCurrencyMarker, Mon, QuoteCurrency};
pub use timestamp_ns::TimestampNs;

/// Natural Logarithmic Returns newtype wrapping a borrowed slice of generic floats.
pub struct LnReturns<'a, T: num_traits::Float>(pub &'a [T]);

/// The user balances denoted in the margin currency.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UserBalances<BaseOrQuote> {
    /// The available wallet balance that is used to provide margin for positions and orders.
    pub available_wallet_balance: BaseOrQuote,
    /// The margin reserved for the position.
    pub position_margin: BaseOrQuote,
    /// The margin reserved for the open limit orders.
    pub order_margin: BaseOrQuote,
}

/// One basis point is 1 / 10_000.
/// It needs to be able to represent 4 decimal places.
pub const BASIS_POINT_SCALE: u8 = 4;

/// One basis point is 1 / 10_000.
/// It needs to be able to represent 4 decimal places.
/// BasisPointFrac::one() is 1 percent.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, derive_more::Add, derive_more::Mul,
)]
#[mul(forward)]
#[repr(transparent)]
pub struct BasisPointFrac(Decimal<i32, 4>);

impl AsRef<Decimal<i32, 4>> for BasisPointFrac {
    #[inline]
    fn as_ref(&self) -> &Decimal<i32, 4> {
        &self.0
    }
}

impl From<Decimal<i32, 4>> for BasisPointFrac {
    #[inline]
    fn from(value: Decimal<i32, 4>) -> Self {
        Self(value)
    }
}

impl num_traits::Zero for BasisPointFrac {
    #[inline]
    fn zero() -> Self {
        Self(Decimal::zero())
    }

    #[inline]
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl num_traits::One for BasisPointFrac {
    #[inline]
    fn one() -> Self {
        Self(Decimal::one())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basis_points_one() {
        // BasisPointFrac::one() is 1 percent.
        assert_eq!(BasisPointFrac::one(), Decimal::try_from_parts(10_000, 0));
    }
}
