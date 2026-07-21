use std::num::NonZeroU16;

use const_decimal::Decimal;
use getset::{
    CopyGetters,
    Getters,
};
use num::Zero;

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
        (self.active_limit_orders.bids().notional_sum()
            + self.active_limit_orders.asks().notional_sum())
            * self.maker_fee.max(Decimal::zero())
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
        self.active_limit_orders.largest_collateral_contributor(
            self.init_margin_req,
            &self.position,
            self.maker_fee,
        )
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

    /// Change the account position, modifying its balances.
    /// This method is usually called by `Exchange`, but exposed for advanced use cases.
    #[inline(always)]
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

    /// Try to insert a new limit order.
    #[inline(always)]
    pub fn try_insert_order(
        &mut self,
        order: LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>,
    ) -> Result<(), MaxNumberOfActiveOrders> {
        self.active_limit_orders.try_insert(order)
    }

    // TODO: this API in princible only need to know `id` (and maybe `side`)
    /// Fill an existing limit order and change the position accordingly; reduces order margin.
    ///
    /// # Panics:
    /// panics if the order id was not found.
    #[inline(always)]
    #[must_use]
    pub fn fill_best(
        &mut self,
        side: Side,
        filled_quantity: BaseOrQuote,
        limit_price: QuoteCurrency<I, D>,
        fee: BaseOrQuote::PairedCurrency,
        ts_ns: TimestampNs,
    ) -> Option<LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>> {
        self.change_position(filled_quantity, limit_price, side, fee);
        self.active_limit_orders
            .fill_best(side, filled_quantity, ts_ns)
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
