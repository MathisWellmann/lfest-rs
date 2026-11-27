use std::num::NonZeroU16;

use const_decimal::Decimal;
use getset::{
    CopyGetters,
    Getters,
};
use num::Zero;
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};

use super::Balances;
use crate::{
    prelude::{
        ActiveLimitOrders,
        Position,
    },
    types::{
        CancelBy,
        Currency,
        Filled,
        FundingRate,
        LimitOrder,
        MarginCurrency,
        MaxNumberOfActiveOrders,
        Mon,
        OrderId,
        OrderIdNotFound,
        Pending,
        QuoteCurrency,
        Side,
        TimestampNs,
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
#[derive(Debug, Clone, CopyGetters, Getters)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "I: Serialize, BaseOrQuote: Serialize, UserOrderIdT: Serialize, BaseOrQuote::PairedCurrency: Serialize",
        deserialize = "I: Deserialize<'de>, BaseOrQuote: Deserialize<'de>, UserOrderIdT: Deserialize<'de>, BaseOrQuote::PairedCurrency: Deserialize<'de>"
    ))
)]
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

    /// The initial margin requirement is set based on the selected leverage of the account.
    #[getset(get_copy = "pub")]
    init_margin_req: Decimal<I, D>,

    /// The maker fee rate of the venue, used to reserve fees for resting limit orders.
    maker_fee: Decimal<I, D>,

    /// The active limit orders of the account.
    #[getset(get = "pub")]
    active_limit_orders: ActiveLimitOrders<I, D, BaseOrQuote, UserOrderIdT>,
}

/// A fully filled limit order.
type FilledLimitOrder<I, const D: u8, BaseOrQuote, UserOrderIdT> =
    LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>;

impl<I, const D: u8, BaseOrQuote, UserOrderIdT> Account<I, D, BaseOrQuote, UserOrderIdT>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Create a new instance with a maximum capacity of `max_active_orders`.
    ///
    /// `init_margin_req` is the initial margin requirement derived from the configured
    /// leverage and `maker_fee` is the venue's maker fee rate, both of which enter the
    /// canonical collateral requirement of the account.
    pub fn new(
        balances: Balances<I, D, BaseOrQuote::PairedCurrency>,
        max_active_orders: NonZeroU16,
        init_margin_req: Decimal<I, D>,
        maker_fee: Decimal<I, D>,
    ) -> Self {
        assert2::assert!(init_margin_req > Decimal::zero());
        assert2::assert!(init_margin_req <= Decimal::ONE);
        Self {
            active_limit_orders: ActiveLimitOrders::with_capacity(max_active_orders),
            position: Position::default(),
            balances,
            init_margin_req,
            maker_fee,
        }
    }

    /// The maker fees reserved for the resting limit orders, so that any of their fills
    /// can always be paid for. A negative (rebate) maker fee reserves nothing.
    #[inline(always)]
    #[must_use]
    pub fn reserved_maker_fees(&self) -> BaseOrQuote::PairedCurrency {
        self.active_limit_orders.reserved_maker_fees(self.maker_fee)
    }

    /// The canonical collateral requirement of the account:
    /// the position margin, the order margin and the reserved maker fees.
    ///
    /// Both order admission in the risk engine and the exchange's post-fill margin
    /// reconciliation are based on this single requirement.
    #[inline(always)]
    #[must_use]
    pub fn required_collateral(&self) -> BaseOrQuote::PairedCurrency {
        self.position_margin() + self.order_margin() + self.reserved_maker_fees()
    }

    /// The signed difference between the account equity and its required collateral.
    ///
    /// A negative value is a collateral deficit: it can arise from settling a
    /// position-reducing fill (which is never rejected) and is resolved by the
    /// exchange's margin call, which force-cancels resting limit orders.
    #[inline(always)]
    #[must_use]
    pub fn margin_excess(&self) -> BaseOrQuote::PairedCurrency {
        self.balances.equity() - self.required_collateral()
    }

    /// The balance available for new orders and positions:
    /// the account equity exceeding the required collateral, floored at zero.
    #[inline(always)]
    #[must_use]
    pub fn available_balance(&self) -> BaseOrQuote::PairedCurrency {
        self.margin_excess().max(Zero::zero())
    }

    /// The id of the resting order whose cancellation frees the most collateral;
    /// see `ActiveLimitOrders::largest_collateral_contributor`.
    #[inline(always)]
    #[must_use]
    pub(crate) fn largest_collateral_contributor(&self) -> Option<OrderId> {
        self.active_limit_orders
            .largest_collateral_contributor(self.init_margin_req, &self.position, self.maker_fee)
            .map(|(order_id, _)| order_id)
    }

    /// The margin excess if `new_order` were also resting,
    /// including its order margin and maker fee reserve.
    #[inline(always)]
    #[must_use]
    pub(crate) fn margin_excess_with_order(
        &self,
        new_order: &LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> BaseOrQuote::PairedCurrency {
        let new_fee_reserve = new_order.notional() * self.maker_fee.max(Decimal::zero());
        self.balances.equity()
            - self.position_margin()
            - self.order_margin_with_order(new_order)
            - self.reserved_maker_fees()
            - new_fee_reserve
    }

    /// The current position margin
    #[inline(always)]
    #[must_use]
    pub fn position_margin(&self) -> BaseOrQuote::PairedCurrency {
        self.position.notional() * self.init_margin_req
    }

    /// The current order margin.
    #[inline(always)]
    #[must_use]
    pub fn order_margin(&self) -> BaseOrQuote::PairedCurrency {
        self.active_limit_orders
            .order_margin(self.init_margin_req, &self.position)
    }

    #[inline(always)]
    #[must_use]
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

    /// Compute the signed perpetual funding cashflow in margin currency for the current
    /// position at `mark_price` and `rate`.
    ///
    /// Positive `rate` means longs pay shorts (`cashflow < 0` for longs, `cashflow > 0`
    /// for shorts); negative `rate` reverses the direction. Returns zero when the position
    /// is neutral or the rate is zero.
    ///
    /// Works for both linear futures (`QuoteCurrency` margin, `qty * mark_price * -rate`)
    /// and inverse futures (`BaseCurrency` margin, `qty / mark_price * -rate`).
    ///
    /// # Panics
    /// Panics if `mark_price <= 0`.
    #[inline]
    #[must_use]
    pub fn funding_cashflow(
        &self,
        rate: FundingRate<I, D>,
        mark_price: QuoteCurrency<I, D>,
    ) -> BaseOrQuote::PairedCurrency {
        self.position.funding_cashflow(rate, mark_price)
    }

    /// Apply a signed funding cashflow directly to the account balances.
    ///
    /// This method is usually called by [`crate::prelude::Exchange::apply_funding`], which
    /// also reconciles margin after settling funding, but is exposed for advanced use cases.
    #[inline(always)]
    pub fn apply_funding(&mut self, cashflow: BaseOrQuote::PairedCurrency) {
        self.balances.apply_funding(cashflow);
    }

    /// Change the account position, modifying its balances.
    /// Returns the nominal realized pnl (excluding `fee`), so the venue can journal pnl and
    /// fee separately.
    #[inline]
    #[must_use]
    pub(crate) fn change_position(
        &mut self,
        filled_qty: BaseOrQuote,
        fill_price: QuoteCurrency<I, D>,
        side: Side,
        fee: BaseOrQuote::PairedCurrency,
    ) -> BaseOrQuote::PairedCurrency {
        assert2::debug_assert!(filled_qty > BaseOrQuote::zero());
        assert2::debug_assert!(fill_price > QuoteCurrency::zero());

        let realized = self.position.change_realizing(filled_qty, fill_price, side);
        self.balances.apply_pnl(realized);
        self.balances.account_for_fee(fee);
        realized
    }

    /// Charge a fee which is not tied to a fill, e.g. the liquidation clearance fee.
    #[inline]
    pub(crate) fn account_for_fee(&mut self, fee: BaseOrQuote::PairedCurrency) {
        assert2::debug_assert!(fee >= BaseOrQuote::PairedCurrency::zero());
        self.balances.account_for_fee(fee);
    }

    /// Try to insert a new limit order.
    #[inline(always)]
    pub fn try_insert_order(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Result<(), MaxNumberOfActiveOrders> {
        self.active_limit_orders.try_insert(order)
    }

    /// Fill the best resting order on `side` and change the position accordingly.
    /// Returns the nominal realized pnl (excluding `fee`) and the order if it is now fully
    /// filled.
    #[inline]
    #[must_use]
    pub(crate) fn fill_best(
        &mut self,
        side: Side,
        filled_quantity: BaseOrQuote,
        limit_price: QuoteCurrency<I, D>,
        fee: BaseOrQuote::PairedCurrency,
        ts_ns: TimestampNs,
    ) -> (
        BaseOrQuote::PairedCurrency,
        Option<FilledLimitOrder<I, D, BaseOrQuote, UserOrderIdT>>,
    ) {
        let realized = self.change_position(filled_quantity, limit_price, side, fee);
        let filled = self
            .active_limit_orders
            .fill_best(side, filled_quantity, ts_ns);
        (realized, filled)
    }

    /// Cancel all active limit orders on `side`, or both sides when `side` is `None`.
    #[must_use]
    pub fn cancel_all_limit_orders(
        &mut self,
        side: Option<Side>,
    ) -> Vec<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>> {
        self.active_limit_orders.cancel_all(side)
    }

    /// Cancel the resting `reduce_only` orders which no longer fit within the open
    /// position, see [`ActiveLimitOrders::cancel_excess_reduce_only`].
    pub(crate) fn cancel_excess_reduce_only_orders<F>(&mut self, on_cancel: F)
    where
        F: FnMut(LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>),
    {
        self.active_limit_orders
            .cancel_excess_reduce_only(self.position.quantity(), on_cancel);
    }

    /// Remove a limit order.
    #[allow(clippy::complexity, reason = "How is this hard to read?")]
    #[inline(always)]
    pub fn cancel_limit_order(
        &mut self,
        by: CancelBy<UserOrderIdT>,
    ) -> Result<
        LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
        OrderIdNotFound<UserOrderIdT>,
    > {
        self.active_limit_orders.remove_limit_order(by)
    }
}
