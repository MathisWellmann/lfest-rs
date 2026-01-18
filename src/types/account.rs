use std::num::NonZeroU16;

use const_decimal::Decimal;
use getset::Getters;
use num::Zero;

use crate::{
    prelude::{
        ActiveLimitOrders,
        Position,
    },
    types::{
        Balances,
        CancelBy,
        Currency,
        LimitOrder,
        MarginCurrency,
        MaxNumberOfActiveOrders,
        Mon,
        OrderIdNotFound,
        Pending,
        QuoteCurrency,
        Side,
        UserOrderId,
    },
};

/// Relevant information about the traders account.
///
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currencies.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
/// - `UserOrderIdT`: The type of user order id to use. Set to `()` if you don't need one.
#[derive(Debug, Clone, Getters)]
pub struct Account<I, const D: u8, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// The current position of the account.
    #[getset(get = "pub")]
    position: Position<I, D, BaseOrQuote>,

    /// The account balances of the account.
    #[getset(get = "pub")]
    balances: Balances<I, D, BaseOrQuote::PairedCurrency>,

    /// The active limit orders of the account.
    #[getset(get = "pub")]
    active_limit_orders: ActiveLimitOrders<I, D, BaseOrQuote, UserOrderIdT>,
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> Account<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Create a new instance with a maximum capacity of `max_active_orders`.
    pub fn new(
        balances: Balances<I, D, BaseOrQuote::PairedCurrency>,
        max_active_orders: NonZeroU16,
    ) -> Self {
        Self {
            active_limit_orders: ActiveLimitOrders::with_capacity(max_active_orders),
            position: Position::Neutral,
            balances,
        }
    }
    /// Change the account position, modifying its balances.
    /// This method is usually called by `Exchange`, but exposed for advanced use cases.
    #[inline]
    pub fn change_position(
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

    /// Get the current order margin.
    #[inline]
    pub fn order_margin(&self, init_margin_req: Decimal<I, D>) -> BaseOrQuote::PairedCurrency {
        self.active_limit_orders
            .order_margin(init_margin_req, &self.position)
    }

    #[deprecated]
    #[inline]
    pub(crate) fn free_order_margin(&mut self, margin: BaseOrQuote::PairedCurrency) {
        self.balances.free_order_margin(margin)
    }

    #[inline]
    pub(crate) fn order_margin_with_order(
        &self,
        new_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        init_margin_req: Decimal<I, D>,
    ) -> BaseOrQuote::PairedCurrency {
        self.active_limit_orders
            .order_margin_with_order(new_order, init_margin_req, &self.position)
    }

    /// Try to insert a new limit order and update the order margin and balances appropriately.
    #[inline]
    pub fn try_insert_order(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        init_margin_req: Decimal<I, D>,
    ) -> Result<(), MaxNumberOfActiveOrders> {
        self.active_limit_orders.try_insert(
            order,
            &self.position,
            &mut self.balances,
            init_margin_req,
        )
    }

    /// fill an existing limit order, reduces order margin.
    /// # Panics:
    /// panics if the order id was not found.
    #[inline]
    pub fn fill_order(
        &mut self,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        init_margin_req: Decimal<I, D>,
    ) {
        self.active_limit_orders.fill_order(
            order,
            &self.position,
            &mut self.balances,
            init_margin_req,
        )
    }

    #[allow(clippy::complexity, reason = "How is this hard to read?")]
    #[inline]
    pub(crate) fn remove_limit_order(
        &mut self,
        by: CancelBy<UserOrderIdT>,
        init_margin_req: Decimal<I, D>,
    ) -> Result<
        LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        OrderIdNotFound<UserOrderIdT>,
    > {
        self.active_limit_orders.remove_limit_order(
            by,
            init_margin_req,
            &self.position,
            &mut self.balances,
        )
    }
}
