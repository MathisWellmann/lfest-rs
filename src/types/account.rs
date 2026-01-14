use const_decimal::Decimal;
use getset::Getters;
use num::Zero;
use typed_builder::TypedBuilder;

use crate::{
    prelude::Position,
    types::{
        Balances,
        Currency,
        MarginCurrency,
        Mon,
        QuoteCurrency,
        Side,
    },
};

/// Relevant information about the traders account.
///
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currencies.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
/// - `UserOrderId`: The type of user order id to use. Set to `()` if you don't need one.
#[derive(Debug, Clone, Getters, TypedBuilder)]
pub struct Account<I, const D: u8, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    /// The current position of the account.
    #[getset(get = "pub")]
    #[builder(default)]
    position: Position<I, D, BaseOrQuote>,

    /// The account balances of the account.
    #[getset(get = "pub")]
    balances: Balances<I, D, BaseOrQuote::PairedCurrency>,
}

impl<I, const D: u8, BaseOrQuote> Account<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    pub(crate) fn change_position(
        &mut self,
        filled_qty: BaseOrQuote,
        fill_price: QuoteCurrency<I, D>,
        side: Side,
        fee: BaseOrQuote::PairedCurrency,
        init_margin_req: Decimal<I, D>,
    ) {
        assert2::debug_assert!(filled_qty > BaseOrQuote::zero());
        assert2::debug_assert!(fill_price > QuoteCurrency::zero());

        self.position.change(
            filled_qty,
            fill_price,
            side,
            &mut self.balances,
            init_margin_req,
        );
        self.balances.account_for_fee(fee);
    }

    #[inline]
    #[must_use]
    pub(crate) fn try_reserve_order_margin(&mut self, margin: BaseOrQuote::PairedCurrency) -> bool {
        self.balances.try_reserve_order_margin(margin)
    }

    #[inline]
    pub(crate) fn free_order_margin(&mut self, margin: BaseOrQuote::PairedCurrency) {
        self.balances.free_order_margin(margin)
    }
}
