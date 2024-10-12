// mod currency;
// mod currency_trait;
mod errors;
mod fee;
mod leverage;
mod limit_order;
mod margin_currency_trait;
mod market_order;
mod order_id;
mod order_meta;
mod order_status;
mod order_update;
mod re_pricing;
mod side;
mod smol_currency;
mod timestamp_ns;

// pub use currency::{BaseCurrency, Currency, MarginCurrency, QuoteCurrency};
// pub use currency_trait::Currency;
pub use errors::*;
pub use fee::{Fee, Maker, Taker};
pub use leverage::Leverage;
pub use limit_order::LimitOrder;
pub use margin_currency_trait::MarginCurrencyMarker;
pub use market_order::MarketOrder;
pub use order_id::OrderId;
pub use order_meta::ExchangeOrderMeta;
pub use order_status::{Filled, FilledQuantity, NewOrder, Pending};
pub use order_update::LimitOrderUpdate;
pub use re_pricing::RePricing;
pub use side::Side;
pub use smol_currency::{Base, CurrencyMarker, Mon, Monies, Quote};
pub use timestamp_ns::TimestampNs;

/// Natural Logarithmic Returns newtype wrapping a borrowed slice of generic floats.
#[deprecated]
pub struct LnReturns<'a, T: num_traits::Float>(pub &'a [T]);

/// The user balances.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UserBalances<T, BaseOrQuote>
where
    T: Mon,
    BaseOrQuote: MarginCurrencyMarker<T>,
{
    /// The available wallet balance that is used to provide margin for positions and orders.
    pub available_wallet_balance: Monies<T, BaseOrQuote>,
    /// The margin reserved for the position.
    pub position_margin: Monies<T, BaseOrQuote>,
    /// The margin reserved for the open limit orders.
    pub order_margin: Monies<T, BaseOrQuote>,
}
