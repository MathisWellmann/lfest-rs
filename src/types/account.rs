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

    /// The initial margin requirement is set based on the selected leverage of the account.
    init_margin_req: Decimal<I, D>,
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
            init_margin_req: Decimal::ONE, // 1x leverage by default.
        }
    }

    /// The available balance is the account equity minus position and order margin.
    #[inline]
    pub fn available_balance(&self) -> BaseOrQuote::PairedCurrency {
        let avail = self.balances.equity() - self.position_margin() - self.order_margin();
        assert!(avail >= Zero::zero());
        avail
    }

    /// The current position margin
    #[inline(always)]
    pub fn position_margin(&self) -> BaseOrQuote::PairedCurrency {
        self.position.notional() * self.init_margin_req
    }

    /// The current order margin.
    #[inline]
    pub fn order_margin(&self) -> BaseOrQuote::PairedCurrency {
        self.active_limit_orders
            .order_margin(self.init_margin_req, &self.position)
    }

    #[inline]
    pub(crate) fn order_margin_with_order(
        &self,
        new_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> BaseOrQuote::PairedCurrency {
        self.active_limit_orders.order_margin_with_order(
            new_order,
            self.init_margin_req,
            &self.position,
        )
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
    ) {
        assert2::debug_assert!(filled_qty > BaseOrQuote::zero());
        assert2::debug_assert!(fill_price > QuoteCurrency::zero());

        self.position
            .change(filled_qty, fill_price, side, &mut self.balances);
        self.balances.account_for_fee(fee);
    }

    /// Try to insert a new limit order and update the order margin and balances appropriately.
    #[inline]
    pub fn try_insert_order(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Result<(), MaxNumberOfActiveOrders> {
        self.active_limit_orders.try_insert(order)
    }

    /// fill an existing limit order, reduces order margin.
    /// # Panics:
    /// panics if the order id was not found.
    #[inline]
    pub fn fill_order(
        &mut self,
        order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) {
        self.active_limit_orders.fill_order(order)
    }

    // TODO: is this a remove due to cancellation, or full fill?
    /// Remove a limit order.
    #[allow(clippy::complexity, reason = "How is this hard to read?")]
    #[inline]
    pub fn remove_limit_order(
        &mut self,
        by: CancelBy<UserOrderIdT>,
    ) -> Result<
        LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        OrderIdNotFound<UserOrderIdT>,
    > {
        self.active_limit_orders.remove_limit_order(by)
    }
}
