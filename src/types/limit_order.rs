use std::cmp::Ordering;

use getset::{
    CopyGetters,
    Getters,
    Setters,
};
use num_traits::Zero;
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};

use super::{
    Currency,
    Filled,
    FilledQuantity,
    MarginCurrency,
    Mon,
    OrderId,
    Pending,
    QuoteCurrency,
    RePricing,
    TimeInForce,
    UserOrderId,
    order_meta::ExchangeOrderMeta,
    order_status::NewOrder,
};
use crate::{
    types::{
        NewLimitOrderError,
        OrderQuantityLTEZero,
        Side,
        TimestampNs,
    },
    utils::NoUserOrderId,
};

/// Defines a limit order.
///
/// Generics:
/// - `I`: The numeric data type of currencies.
/// - `D`: The constant decimal precision of the currencies.
/// - `BaseOrQuote`: Either `BaseCurrency` or `QuoteCurrency` depending on the futures type.
/// - `UserOrderId`: The type of user order id to use. Set to `()` if you don't need one.
/// - `OrderStatus`: The status of the order for each stage, contains different information based on the stage.
#[derive(Debug, Clone, PartialEq, Eq, Getters, CopyGetters, Setters)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LimitOrder<I, const D: u8, BaseOrQuote, UserOrderIdT, OrderStatus>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Order Id provided by the user, can be any type really.
    #[getset(get_copy = "pub")]
    user_order_id: UserOrderIdT,

    /// Whether its a buy or sell order.
    #[getset(get_copy = "pub")]
    side: Side,

    /// The limit order price, where it will sit in the order book.
    #[getset(get_copy = "pub")]
    limit_price: QuoteCurrency<I, D>,

    /// The remaining amount of Currency `S` the order is for.
    #[getset(get_copy = "pub")]
    remaining_quantity: BaseOrQuote,

    /// Time-in-force policy governing immediate execution vs resting behavior.
    ///
    /// The single source of the order's crossing behavior: the [`RePricing`] view
    /// ([`LimitOrder::re_pricing`]) is derived from it, so the two can never contradict.
    #[getset(get_copy = "pub", set = "pub")]
    time_in_force: TimeInForce,

    /// When `true`, the order may only reduce an existing opposite-side position.
    #[getset(get_copy = "pub", set = "pub")]
    reduce_only: bool,

    /// Depending on the status, different information is available.
    #[getset(get = "pub")]
    state: OrderStatus,
}

/// Price time priority ordering
/// Lower priced orders will come first,
/// at the same price, older orders come first.
impl<I, const D: u8, BaseOrQuote, UserOrderIdT> PartialOrd
    for LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Price time priority ordering
/// Lower priced orders will come first,
/// at the same price, older orders come first.
impl<I, const D: u8, BaseOrQuote, UserOrderIdT> Ord
    for LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    #[inline(always)]
    fn cmp(&self, other: &Self) -> Ordering {
        self.limit_price().cmp(&other.limit_price()).then_with(|| {
            self.state()
                .meta()
                .ts_exchange_received()
                .cmp(&other.state().meta().ts_exchange_received())
        })
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT, OrderStatus> std::fmt::Display
    for LimitOrder<I, D, BaseOrQuote, UserOrderIdT, OrderStatus>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
    OrderStatus: Clone + std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "user_id: {:?}, limit {} {} @ {}, state: {:?}",
            self.user_order_id, self.side, self.remaining_quantity, self.limit_price, self.state
        )
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT, OrderStatus>
    LimitOrder<I, D, BaseOrQuote, UserOrderIdT, OrderStatus>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// What the order does when its limit price locks or crosses the opposite quote,
    /// derived from [`LimitOrder::time_in_force`]: only `GoodTilCrossing` is
    /// [`RePricing::GoodTilCrossing`] (post-only), every other time in force is
    /// [`RePricing::Marketable`].
    #[inline]
    #[must_use]
    pub fn re_pricing(&self) -> RePricing {
        match self.time_in_force {
            TimeInForce::GoodTilCrossing => RePricing::GoodTilCrossing,
            TimeInForce::GoodTilCancelled
            | TimeInForce::ImmediateOrCancel
            | TimeInForce::FillOrKill => RePricing::Marketable,
        }
    }

    /// `true` for post-only orders, which are rejected instead of executed when crossing.
    #[inline]
    #[must_use]
    pub fn is_post_only(&self) -> bool {
        self.re_pricing() == RePricing::GoodTilCrossing
    }
}

/// The time in force expressing `re_pricing` for an order currently at `current`.
#[inline]
fn time_in_force_for(re_pricing: RePricing, current: TimeInForce) -> TimeInForce {
    match (re_pricing, current) {
        (RePricing::GoodTilCrossing, _) => TimeInForce::GoodTilCrossing,
        (RePricing::Marketable, TimeInForce::GoodTilCrossing) => TimeInForce::GoodTilCancelled,
        (RePricing::Marketable, marketable) => marketable,
    }
}

impl<I, const D: u8, BaseOrQuote> LimitOrder<I, D, BaseOrQuote, NoUserOrderId, NewOrder>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
{
    /// Create a new limit order without a user_order_id.
    ///
    /// # Arguments:
    /// - `side`: either buy or sell
    /// - `limit_price`: price to execute at or better
    /// - `quantity`: A positive nonzero quantity of the amount of contracts this order is for.
    ///
    /// # Returns:
    /// Either a successfully created order or a well scoped [`NewLimitOrderError`].
    pub fn new(
        side: Side,
        limit_price: QuoteCurrency<I, D>,
        quantity: BaseOrQuote,
    ) -> Result<Self, NewLimitOrderError> {
        if limit_price <= QuoteCurrency::zero() {
            return Err(NewLimitOrderError::LimitPriceLTEZero);
        }
        if quantity <= BaseOrQuote::zero() {
            return Err(OrderQuantityLTEZero.into());
        }
        Ok(Self {
            user_order_id: NoUserOrderId,
            state: NewOrder,
            limit_price,
            remaining_quantity: quantity,
            side,
            time_in_force: TimeInForce::default(),
            reduce_only: false,
        })
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT>
    LimitOrder<I, D, BaseOrQuote, UserOrderIdT, NewOrder>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Create a new limit order
    ///
    /// # Arguments:
    /// - `side`: either buy or sell
    /// - `limit_price`: price to execute at or better
    /// - `quantity`: How many contracts should be traded
    /// - `user_order_id`: The user provided id. This value is ignored by the exchange.
    ///
    /// # Returns:
    /// Either a successfully created order or a well scoped [`NewLimitOrderError`]
    pub fn new_with_user_order_id(
        side: Side,
        limit_price: QuoteCurrency<I, D>,
        quantity: BaseOrQuote,
        user_order_id: UserOrderIdT,
    ) -> Result<Self, NewLimitOrderError> {
        if limit_price <= QuoteCurrency::zero() {
            return Err(NewLimitOrderError::LimitPriceLTEZero);
        }
        if quantity <= BaseOrQuote::zero() {
            return Err(OrderQuantityLTEZero.into());
        }
        Ok(Self {
            user_order_id,
            state: NewOrder,
            limit_price,
            remaining_quantity: quantity,
            side,
            time_in_force: TimeInForce::default(),
            reduce_only: false,
        })
    }

    /// Set the [`TimeInForce`] (builder-style).
    ///
    /// The time in force alone decides the crossing behavior, so this also determines
    /// [`LimitOrder::re_pricing`]: [`RePricing::GoodTilCrossing`] for `GoodTilCrossing`,
    /// [`RePricing::Marketable`] otherwise. The last builder call wins.
    #[inline]
    #[must_use]
    pub fn with_time_in_force(mut self, tif: TimeInForce) -> Self {
        self.time_in_force = tif;
        self
    }

    /// Set the crossing behavior via [`RePricing`] (builder-style), expressed as the
    /// equivalent [`TimeInForce`] because that is the only stored policy:
    ///
    /// - [`RePricing::GoodTilCrossing`] makes the order post-only (`GoodTilCrossing`),
    ///   whatever time in force was set before.
    /// - [`RePricing::Marketable`] turns a post-only order into `GoodTilCancelled` and
    ///   keeps `GoodTilCancelled`, `ImmediateOrCancel` and `FillOrKill`, which are
    ///   all marketable already.
    ///
    /// The last builder call wins, so `with_time_in_force` and `with_re_pricing` can be
    /// chained in either order without leaving a contradictory order behind.
    #[inline]
    #[must_use]
    pub fn with_re_pricing(mut self, re_pricing: RePricing) -> Self {
        self.set_re_pricing(re_pricing);
        self
    }

    /// Set the crossing behavior via [`RePricing`]; see [`LimitOrder::with_re_pricing`].
    #[inline]
    pub fn set_re_pricing(&mut self, re_pricing: RePricing) {
        self.time_in_force = time_in_force_for(re_pricing, self.time_in_force);
        assert_eq!(self.re_pricing(), re_pricing);
    }

    /// Mark this limit order as reduce-only (builder-style).
    #[inline]
    #[must_use]
    pub fn with_reduce_only(mut self, reduce_only: bool) -> Self {
        self.reduce_only = reduce_only;
        self
    }

    /// Set the `UserOrderId`.
    #[inline]
    pub fn set_user_order_id(&mut self, user_order_id: UserOrderIdT) {
        self.user_order_id = user_order_id;
    }

    /// Get the total quantity that this order is for.
    #[inline]
    pub fn total_quantity(&self) -> BaseOrQuote {
        self.remaining_quantity
    }

    /// Take in the order metadata provided by the exchange and coverts the order to the `Pending` state.
    pub fn into_pending(
        self,
        meta: ExchangeOrderMeta,
    ) -> LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>> {
        LimitOrder {
            user_order_id: self.user_order_id,
            side: self.side,
            limit_price: self.limit_price,
            remaining_quantity: self.remaining_quantity,
            state: Pending::new(meta),
            time_in_force: self.time_in_force,
            reduce_only: self.reduce_only,
        }
    }

    /// `true` if the order crosses (or locks) the top of book of `market_state`.
    #[inline]
    #[must_use]
    pub(crate) fn is_marketable(
        &self,
        market_state: &crate::market_state::MarketState<I, D>,
    ) -> bool {
        match self.side {
            Side::Buy => self.limit_price >= market_state.ask(),
            Side::Sell => self.limit_price <= market_state.bid(),
        }
    }

    /// Modify the `remaining_quantity`.
    /// The `new_qty` must be GT than zero.
    pub(crate) fn set_remaining_quantity(&mut self, new_qty: BaseOrQuote) {
        assert!(new_qty > BaseOrQuote::zero());
        self.remaining_quantity = new_qty;
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT>
    LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Pending<I, D, BaseOrQuote>>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Used when an order gets some `quantity` filled at its limit price.
    #[inline(always)]
    pub fn fill(&mut self, filled_quantity: BaseOrQuote) {
        assert2::debug_assert!(
            filled_quantity <= self.remaining_quantity,
            "The filled quantity can not be greater than the limit order quantity"
        );
        assert2::debug_assert!(
            filled_quantity > BaseOrQuote::zero(),
            "Filled quantity must be greater than zero."
        );

        self.remaining_quantity -= filled_quantity;
        assert2::debug_assert!(
            self.remaining_quantity >= Zero::zero(),
            "Quantity must be positive"
        );

        use FilledQuantity::*;
        match self.state.filled_quantity_mut() {
            Unfilled => {
                self.state.set_filled_quantity(Filled {
                    cumulative_qty: filled_quantity,
                    avg_price: self.limit_price,
                });
            }
            Filled {
                cumulative_qty,
                avg_price: _,
            } => {
                *cumulative_qty += filled_quantity;
            }
        };
    }

    /// Transition into the `Filled` state.
    /// # Panics
    /// When the `remaining_quanity > 0`
    #[inline(always)]
    pub fn into_filled(
        self,
        ts_ns: TimestampNs,
    ) -> LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>> {
        debug_assert!(self.remaining_quantity.is_zero());
        let cumulative_qty = match self.state.filled_quantity() {
            FilledQuantity::Filled {
                cumulative_qty,
                avg_price: _,
            } => *cumulative_qty,
            FilledQuantity::Unfilled => {
                unreachable!("Can only transition to Filled state if remaining quantity is zero")
            }
        };
        LimitOrder {
            user_order_id: self.user_order_id,
            state: Filled::new(
                self.state.meta().clone(),
                ts_ns,
                self.limit_price(),
                cumulative_qty,
            ),
            limit_price: self.limit_price,
            remaining_quantity: BaseOrQuote::zero(),
            side: self.side,
            time_in_force: self.time_in_force,
            reduce_only: self.reduce_only,
        }
    }

    /// Transition an immediately filled marketable limit order into the `Filled` state
    /// at an explicit execution price `fill_price` (e.g. best bid/ask <= limit price).
    #[inline]
    pub(crate) fn into_filled_at_price(
        self,
        fill_price: QuoteCurrency<I, D>,
        ts_ns: TimestampNs,
    ) -> LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>> {
        LimitOrder {
            user_order_id: self.user_order_id,
            state: Filled::new(
                self.state.meta().clone(),
                ts_ns,
                fill_price,
                self.remaining_quantity,
            ),
            limit_price: self.limit_price,
            remaining_quantity: BaseOrQuote::zero(),
            side: self.side,
            time_in_force: self.time_in_force,
            reduce_only: self.reduce_only,
        }
    }

    /// Get the total filled quantity for this order.
    pub fn filled_quantity(&self) -> BaseOrQuote {
        use FilledQuantity::*;
        match self.state.filled_quantity() {
            Unfilled => BaseOrQuote::zero(),
            Filled {
                cumulative_qty,
                avg_price: _,
            } => *cumulative_qty,
        }
    }

    /// Get the total quantity that this order is for.
    pub fn total_quantity(&self) -> BaseOrQuote {
        use FilledQuantity::*;
        let q = match self.state.filled_quantity() {
            Unfilled => self.remaining_quantity,
            Filled {
                cumulative_qty,
                avg_price: _,
            } => self.remaining_quantity + *cumulative_qty,
        };
        assert2::debug_assert!(
            q > BaseOrQuote::zero(),
            "total quantity must always be > zero"
        );
        q
    }

    /// Get the order id assigned by the exchange.
    #[inline(always)]
    pub fn id(&self) -> OrderId {
        self.state().meta().id()
    }

    /// The notional value is related to the quantity and its limit price.
    #[inline(always)]
    pub fn notional(&self) -> BaseOrQuote::PairedCurrency {
        BaseOrQuote::PairedCurrency::convert_from(self.remaining_quantity, self.limit_price)
    }
}

impl<I, const D: u8, BaseOrQuote, UserOrderIdT>
    LimitOrder<I, D, BaseOrQuote, UserOrderIdT, Filled<I, D, BaseOrQuote>>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
    BaseOrQuote::PairedCurrency: MarginCurrency<I, D>,
    UserOrderIdT: UserOrderId,
{
    /// Get the total quantity that this order is for.
    pub fn total_quantity(&self) -> BaseOrQuote {
        let q = self.state.filled_qty();
        assert2::debug_assert!(
            q > BaseOrQuote::zero(),
            "total quantity must always be > zero"
        );
        q
    }

    /// Get the order id assigned by the exchange.
    #[inline]
    pub fn id(&self) -> OrderId {
        self.state().meta().id()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use test_case::test_matrix;

    use super::*;
    use crate::{
        DECIMALS,
        prelude::QuoteCurrency,
        types::BaseCurrency,
    };

    #[test_matrix(
        [Side::Buy, Side::Sell],
        [100, 110, 120],
        [1, 2, 3]
    )]
    fn limit_order_fill_full(side: Side, limit_price: u32, qty: i64) {
        let limit_price = QuoteCurrency::<i64, DECIMALS>::new(limit_price as i64, 0);
        let qty = QuoteCurrency::new(qty, 0);
        let order = LimitOrder::new(side, limit_price, qty).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());

        let mut order = order.into_pending(meta.clone());

        order.fill(qty);
        assert_eq!(order.total_quantity(), qty);
        assert_eq!(order.remaining_quantity(), QuoteCurrency::zero());
    }

    #[test_matrix(
        [Side::Buy, Side::Sell],
        [100, 110, 120],
        [1, 2, 3]
    )]
    fn limit_order_fill_partial(side: Side, limit_price: i64, qty: i64) {
        let limit_price = QuoteCurrency::<i64, DECIMALS>::new(limit_price, 0);
        let quantity = QuoteCurrency::new(qty, 0);
        let order = LimitOrder::new(side, limit_price, quantity).unwrap();
        let meta = ExchangeOrderMeta::new(0.into(), 0.into());

        let qty = quantity / QuoteCurrency::new(2, 0);
        let mut order = order.into_pending(meta.clone());
        order.fill(qty);
        assert_eq!(order.remaining_quantity, quantity - qty);
        assert_eq!(order.total_quantity(), quantity);

        let expected_state = Pending::builder()
            .meta(meta.clone())
            .filled_quantity(FilledQuantity::Filled {
                cumulative_qty: qty,
                avg_price: limit_price,
            })
            .build();
        assert_eq!(order.state(), &expected_state);

        // Now fully fill
        order.fill(qty);
        assert_eq!(order.remaining_quantity, Zero::zero());
        assert_eq!(order.total_quantity(), quantity);
    }

    #[test]
    fn size_of_limit_order() {
        use std::mem::size_of;
        assert_eq!(
            size_of::<LimitOrder<i64, 5, BaseCurrency<i64, 5>, i64, NewOrder>>(),
            32
        );
        assert_eq!(
            size_of::<LimitOrder<i32, 2, BaseCurrency<i32, 2>, i64, NewOrder>>(),
            24
        );
        assert_eq!(
            size_of::<LimitOrder<i32, 2, BaseCurrency<i32, 2>, i32, NewOrder>>(),
            16
        );
        assert_eq!(
            size_of::<
                LimitOrder<
                    i32,
                    2,
                    BaseCurrency<i32, 2>,
                    i64,
                    Pending<i32, 2, BaseCurrency<i32, 2>>,
                >,
            >(),
            56
        );
        assert_eq!(
            size_of::<
                LimitOrder<
                    i64,
                    2,
                    BaseCurrency<i64, 2>,
                    i64,
                    Pending<i64, 2, BaseCurrency<i64, 2>>,
                >,
            >(),
            72
        );
    }

    #[test]
    fn limit_order_new_with_user_order_id() {
        let order = LimitOrder::new_with_user_order_id(
            Side::Buy,
            QuoteCurrency::<i64, 5>::new(100, 0),
            BaseCurrency::new(5, 0),
            1,
        )
        .unwrap();
        assert_eq!(order.user_order_id(), 1);
    }

    type NewBuy = LimitOrder<i64, 5, BaseCurrency<i64, 5>, NoUserOrderId, NewOrder>;

    fn new_buy() -> NewBuy {
        LimitOrder::new(
            Side::Buy,
            QuoteCurrency::new(100, 0),
            BaseCurrency::new(1, 0),
        )
        .unwrap()
    }

    const ALL_TIF: [TimeInForce; 4] = [
        TimeInForce::GoodTilCrossing,
        TimeInForce::GoodTilCancelled,
        TimeInForce::ImmediateOrCancel,
        TimeInForce::FillOrKill,
    ];

    #[test]
    fn default_limit_order_is_post_only() {
        let order = new_buy();
        assert_eq!(order.time_in_force(), TimeInForce::GoodTilCrossing);
        assert_eq!(order.re_pricing(), RePricing::GoodTilCrossing);
        assert!(order.is_post_only());
    }

    #[test]
    fn re_pricing_after_time_in_force_wins() {
        // The reported contradiction: GTC then GTX re-pricing used to keep GTC, so the
        // order claimed to be post-only but was not treated as such.
        let order = new_buy()
            .with_time_in_force(TimeInForce::GoodTilCancelled)
            .with_re_pricing(RePricing::GoodTilCrossing);
        assert_eq!(order.time_in_force(), TimeInForce::GoodTilCrossing);
        assert_eq!(order.re_pricing(), RePricing::GoodTilCrossing);
        assert!(order.is_post_only());

        for tif in ALL_TIF {
            let post_only = new_buy()
                .with_time_in_force(tif)
                .with_re_pricing(RePricing::GoodTilCrossing);
            assert!(post_only.is_post_only(), "{tif:?}");
            assert_eq!(post_only.time_in_force(), TimeInForce::GoodTilCrossing);

            let marketable = new_buy()
                .with_time_in_force(tif)
                .with_re_pricing(RePricing::Marketable);
            assert!(!marketable.is_post_only(), "{tif:?}");
            assert_eq!(marketable.re_pricing(), RePricing::Marketable);
            // A marketable time in force is kept, only post-only becomes GTC.
            let expected = match tif {
                TimeInForce::GoodTilCrossing => TimeInForce::GoodTilCancelled,
                other => other,
            };
            assert_eq!(marketable.time_in_force(), expected);
        }
    }

    #[test]
    fn time_in_force_after_re_pricing_wins() {
        for re_pricing in [RePricing::GoodTilCrossing, RePricing::Marketable] {
            for tif in ALL_TIF {
                let order = new_buy()
                    .with_re_pricing(re_pricing)
                    .with_time_in_force(tif);
                assert_eq!(order.time_in_force(), tif);
                assert_eq!(
                    order.is_post_only(),
                    tif == TimeInForce::GoodTilCrossing,
                    "{re_pricing:?} then {tif:?}"
                );
                assert_eq!(
                    order.re_pricing() == RePricing::GoodTilCrossing,
                    order.is_post_only()
                );
            }
        }
    }

    #[test]
    fn setters_cannot_create_a_contradiction() {
        let mut order = new_buy();
        order.set_time_in_force(TimeInForce::FillOrKill);
        assert_eq!(order.re_pricing(), RePricing::Marketable);
        order.set_re_pricing(RePricing::GoodTilCrossing);
        assert_eq!(order.time_in_force(), TimeInForce::GoodTilCrossing);
        assert!(order.is_post_only());
        order.set_re_pricing(RePricing::Marketable);
        assert_eq!(order.time_in_force(), TimeInForce::GoodTilCancelled);

        // The derived view survives the state transitions.
        let pending = new_buy()
            .with_time_in_force(TimeInForce::ImmediateOrCancel)
            .into_pending(ExchangeOrderMeta::new(0.into(), 0.into()));
        assert_eq!(pending.re_pricing(), RePricing::Marketable);
        assert!(!pending.is_post_only());
    }

    proptest! {
        #[test]
        fn limit_order_fill_proptest_buy_partial(
            filled_qty in 1..4999_i64,
        ) {
            let init_qty = BaseCurrency::new(5000, 4);
            let order =
                LimitOrder::new(Side::Buy, QuoteCurrency::<i64, 5>::new(100, 0), init_qty).unwrap();
            let meta = ExchangeOrderMeta::new(0.into(), 0.into());
            let mut order = order.into_pending(meta.clone());

            let filled_quantity = BaseCurrency::new(filled_qty, 4);
            order.fill(filled_quantity);
            assert_eq!(order.remaining_quantity, init_qty - filled_quantity);
            assert_eq!(order.total_quantity(), init_qty);
        }
    }

    type Qty = BaseCurrency<i64, 5>;
    type NewLimit = LimitOrder<i64, 5, Qty, NoUserOrderId, NewOrder>;

    fn qty(q: i64) -> Qty {
        Qty::new(q, 0)
    }

    fn limit(side: Side, price: i64, q: i64) -> NewLimit {
        LimitOrder::new(side, QuoteCurrency::new(price, 0), qty(q)).expect("valid order")
    }

    #[test]
    fn is_marketable_when_crossing_or_locking_the_touch() {
        let ms = crate::market_state::MarketState::<i64, 5>::from_components(
            QuoteCurrency::new(100, 0),
            QuoteCurrency::new(101, 0),
            QuoteCurrency::new(100, 0),
            1.into(),
            1,
        );
        assert!(limit(Side::Buy, 101, 1).is_marketable(&ms));
        assert!(limit(Side::Buy, 102, 1).is_marketable(&ms));
        assert!(!limit(Side::Buy, 100, 1).is_marketable(&ms));
        assert!(limit(Side::Sell, 100, 1).is_marketable(&ms));
        assert!(limit(Side::Sell, 99, 1).is_marketable(&ms));
        assert!(!limit(Side::Sell, 101, 1).is_marketable(&ms));
    }
}
