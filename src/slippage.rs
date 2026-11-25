//! Pluggable market-impact and slippage models for aggressive executions
//! (adapted from `OrderBook-rs` `market_impact.rs`).

use const_decimal::Decimal;
use getset::CopyGetters;
use num_traits::{
    NumCast,
    One,
    Signed,
    Zero,
};
#[cfg(feature = "serde")]
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    market_state::MarketState,
    types::{
        ConfigError,
        Currency,
        Mon,
        QuoteCurrency,
        Side,
    },
};

/// Structured quote returned by a [`SlippageModel`] for an aggressive order.
#[derive(Debug, Clone, Copy, Eq, PartialEq, CopyGetters)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MarketImpactQuote<I, const D: u8>
where
    I: Mon<D>,
{
    /// Top-of-book reference price before slippage (`ask` for buy, `bid` for sell).
    #[getset(get_copy = "pub")]
    reference_price: QuoteCurrency<I, D>,

    /// Effective average execution price including market impact.
    #[getset(get_copy = "pub")]
    execution_price: QuoteCurrency<I, D>,

    /// Non-negative adverse price impact (`|execution_price - reference_price|`).
    #[getset(get_copy = "pub")]
    slippage: QuoteCurrency<I, D>,

    /// The model's adverse impact rate, rounded up. `slippage` can exceed
    /// `reference_price * slippage_rate` by up to one tick, because the execution price
    /// snaps to the tick grid away from the trader.
    #[getset(get_copy = "pub")]
    slippage_rate: Decimal<I, D>,
}

/// Pluggable execution slippage / market-impact model for aggressive orders.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SlippageModel<I, const D: u8>
where
    I: Mon<D>,
{
    /// Default zero-impact model: fills at exact best ask (Buy) / best bid (Sell).
    #[default]
    ZeroImpact,
    /// Constant fractional adverse slippage applied on top of best bid/ask
    /// (e.g. `0.0005` = 5 bps adverse slippage).
    FixedRate {
        /// Non-negative adverse slippage fraction in `[0, 1)`.
        rate: Decimal<I, D>,
    },
    /// Linear + square-root participation impact model:
    ///
    /// `impact_rate = base_rate + linear_coeff * participation + sqrt_coeff * sqrt(participation)`
    /// where `participation = order_qty / reference_volume`.
    LinearSqrt {
        /// Constant baseline slippage fraction (e.g. half-spread buffer).
        base_rate: Decimal<I, D>,
        /// Linear coefficient on participation ratio (`qty / reference_volume`).
        linear_coeff: Decimal<I, D>,
        /// Square-root (Almgren-Chriss / Kyle) coefficient on `sqrt(qty / reference_volume)`.
        sqrt_coeff: Decimal<I, D>,
        /// Reference bar / ADV volume in contract quantity units (must be > 0).
        reference_volume: Decimal<I, D>,
        /// Hard upper cap on total impact fraction in `(0, 1)`.
        max_impact_rate: Decimal<I, D>,
    },
}

impl<I, const D: u8> SlippageModel<I, D>
where
    I: Mon<D>,
{
    /// Construct a validated [`SlippageModel::FixedRate`].
    ///
    /// # Errors
    /// Returns [`ConfigError::InvalidSlippageModel`] if `rate < 0` or `rate >= 1`.
    pub fn fixed_rate(rate: Decimal<I, D>) -> Result<Self, ConfigError> {
        if rate < Decimal::zero() || rate >= Decimal::one() {
            return Err(ConfigError::InvalidSlippageModel);
        }
        Ok(Self::FixedRate { rate })
    }

    /// Construct a validated [`SlippageModel::LinearSqrt`] model.
    ///
    /// # Errors
    /// Returns [`ConfigError::InvalidSlippageModel`] if any coefficient is negative,
    /// `reference_volume <= 0`, or `max_impact_rate` is outside `(0, 1)`.
    pub fn linear_sqrt(
        base_rate: Decimal<I, D>,
        linear_coeff: Decimal<I, D>,
        sqrt_coeff: Decimal<I, D>,
        reference_volume: Decimal<I, D>,
        max_impact_rate: Decimal<I, D>,
    ) -> Result<Self, ConfigError> {
        if base_rate < Decimal::zero()
            || linear_coeff < Decimal::zero()
            || sqrt_coeff < Decimal::zero()
            || reference_volume <= Decimal::zero()
            || max_impact_rate <= Decimal::zero()
            || max_impact_rate >= Decimal::one()
        {
            return Err(ConfigError::InvalidSlippageModel);
        }
        Ok(Self::LinearSqrt {
            base_rate,
            linear_coeff,
            sqrt_coeff,
            reference_volume,
            max_impact_rate,
        })
    }

    /// Quote the effective execution price and slippage breakdown for a market order
    /// of `(side, quantity)` given `market_state`, on a price grid of `tick_size`.
    ///
    /// All arithmetic is fixed point and every rounding step goes against the trader:
    /// the impact rate and the slippage amount round up, and the execution price snaps
    /// to the tick grid away from the trader (buy: up, sell: down). A sell never executes
    /// below one tick. Rounding in the trader's favour would let a backtest earn money
    /// from the simulator's numerics.
    ///
    /// # Panics
    /// If `tick_size <= 0`, or a model built without its validating constructor holds a
    /// negative coefficient or a non-positive `reference_volume`.
    #[inline]
    #[must_use]
    pub fn quote<BaseOrQuote: Currency<I, D>>(
        &self,
        market_state: &MarketState<I, D>,
        side: Side,
        quantity: BaseOrQuote,
        tick_size: QuoteCurrency<I, D>,
    ) -> MarketImpactQuote<I, D> {
        assert!(
            tick_size > QuoteCurrency::zero(),
            "tick size must be positive"
        );
        let reference_price = match side {
            Side::Buy => market_state.ask(),
            Side::Sell => market_state.bid(),
        };
        let impact_rate = match *self {
            Self::ZeroImpact => Decimal::zero(),
            Self::FixedRate { rate } => rate,
            Self::LinearSqrt {
                base_rate,
                linear_coeff,
                sqrt_coeff,
                reference_volume,
                max_impact_rate,
            } => {
                let rate = linear_sqrt_rate_raw(
                    to_raw(quantity.as_ref().abs()),
                    LinearSqrtRaw {
                        base_rate: to_raw(base_rate),
                        linear_coeff: to_raw(linear_coeff),
                        sqrt_coeff: to_raw(sqrt_coeff),
                        reference_volume: to_raw(reference_volume),
                        max_impact_rate: to_raw(max_impact_rate),
                    },
                    to_raw(Decimal::<I, D>::ONE),
                );
                from_raw(rate)
            }
        };
        assert!(impact_rate >= Decimal::zero(), "impact rate must be >= 0");

        if impact_rate.is_zero() || reference_price <= QuoteCurrency::zero() {
            return MarketImpactQuote {
                reference_price,
                execution_price: reference_price,
                slippage: QuoteCurrency::zero(),
                slippage_rate: impact_rate,
            };
        }

        let reference_raw = to_raw(*reference_price.as_ref());
        // `ceil(reference * rate)`. The product only overflows `u128` for `i128`
        // prices near its range; then one ulp above the floored product is still >= the
        // exact value, i.e. still adverse.
        let slippage_raw = mul_div_ceil(
            reference_raw,
            to_raw(impact_rate),
            to_raw(Decimal::<I, D>::ONE),
        )
        .unwrap_or_else(|| to_raw(*(reference_price * impact_rate).as_ref()) + 1);
        let execution_raw = adverse_execution_price_raw(
            side,
            reference_raw,
            slippage_raw,
            to_raw(*tick_size.as_ref()),
        );
        let execution_price = QuoteCurrency::from(from_raw(execution_raw));
        let slippage = (execution_price - reference_price).abs();
        match side {
            Side::Buy => assert!(execution_price > reference_price),
            Side::Sell => {
                assert!(execution_price < reference_price || reference_price <= tick_size)
            }
        }
        assert!(execution_price > QuoteCurrency::zero());

        MarketImpactQuote {
            reference_price,
            execution_price,
            slippage,
            slippage_rate: impact_rate,
        }
    }
}

/// Raw scaled integers of a [`SlippageModel::LinearSqrt`] model.
#[derive(Debug, Clone, Copy)]
struct LinearSqrtRaw {
    base_rate: u128,
    linear_coeff: u128,
    sqrt_coeff: u128,
    reference_volume: u128,
    max_impact_rate: u128,
}

/// The raw scaled integer of a non-negative decimal, widened to `u128` so the products
/// below cannot overflow for `i32` and `i64` and rarely do for `i128`.
#[inline]
fn to_raw<I: Mon<D>, const D: u8>(value: Decimal<I, D>) -> u128 {
    value
        .0
        .to_u128()
        .expect("slippage inputs are non-negative (validated by the SlippageModel constructors)")
}

/// Narrow a raw scaled integer back into a decimal.
#[inline]
fn from_raw<I: Mon<D>, const D: u8>(raw: u128) -> Decimal<I, D> {
    Decimal(
        <I as NumCast>::from(raw).expect("slippage result exceeds the range of the integer type"),
    )
}

/// `ceil(a * b / d)`, or `None` if `a * b` overflows.
#[inline]
fn mul_div_ceil(a: u128, b: u128, d: u128) -> Option<u128> {
    assert!(d > 0, "divisor must be positive");
    let product = a.checked_mul(b)?;
    let out = product.div_ceil(d);
    assert!(out.checked_mul(d).is_none_or(|back| back >= product));
    Some(out)
}

/// `ceil(sqrt(n))`, exact for perfect squares.
#[inline]
fn isqrt_ceil(n: u128) -> u128 {
    let floor = n.isqrt();
    // `floor <= 2^64 - 1`, so neither the square nor the increment overflows.
    let out = if floor * floor < n { floor + 1 } else { floor };
    assert!(out.checked_mul(out).is_none_or(|sq| sq >= n));
    out
}

/// Impact rate of a [`SlippageModel::LinearSqrt`] model in raw units of `scale`, rounded up.
///
/// `rate = base + linear * p + sqrt_coeff * sqrt(p)` with `p = qty / reference_volume`,
/// capped at `max_impact_rate`. Each term rounds up, and every term is monotone in
/// `qty`, so the result never undercuts the exact rate. An overflowing intermediate
/// means an enormous participation whose rate is far above any valid cap (< 1), so it
/// is capped.
fn linear_sqrt_rate_raw(qty: u128, model: LinearSqrtRaw, scale: u128) -> u128 {
    assert!(
        model.reference_volume > 0,
        "reference volume must be positive"
    );
    assert!(
        model.max_impact_rate > 0,
        "max impact rate must be positive"
    );
    let participation = mul_div_ceil(qty, scale, model.reference_volume);
    let linear = match model.linear_coeff {
        0 => Some(0),
        c => participation.and_then(|p| mul_div_ceil(c, p, scale)),
    };
    // `sqrt(p) * scale == sqrt(p_raw * scale)`.
    let sqrt = match model.sqrt_coeff {
        0 => Some(0),
        c => participation
            .and_then(|p| p.checked_mul(scale))
            .map(isqrt_ceil)
            .and_then(|root| mul_div_ceil(c, root, scale)),
    };
    let total = linear
        .zip(sqrt)
        .and_then(|(l, s)| model.base_rate.checked_add(l)?.checked_add(s));
    let rate = total.map_or(model.max_impact_rate, |t| t.min(model.max_impact_rate));
    assert!(rate <= model.max_impact_rate);
    rate
}

/// Execution price `reference -/+ slippage` snapped to the `tick` grid away from the
/// trader: up for a buy, down for a sell, but never below one tick.
#[inline]
fn adverse_execution_price_raw(side: Side, reference: u128, slippage: u128, tick: u128) -> u128 {
    assert!(tick > 0, "tick size must be positive");
    let out = match side {
        Side::Buy => (reference + slippage).div_ceil(tick) * tick,
        Side::Sell => (reference.saturating_sub(slippage) / tick * tick).max(tick),
    };
    assert_eq!(out % tick, 0, "execution price must lie on the tick grid");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::{
        BaseCurrency,
        Bba,
        PriceFilter,
    };

    fn market(bid: QuoteCurrency<i64, 5>, ask: QuoteCurrency<i64, 5>) -> MarketState<i64, 5> {
        let mut ms = MarketState::<i64, 5>::default();
        ms.update_state::<_, BaseCurrency<i64, 5>>(
            &Bba {
                bid,
                ask,
                timestamp_exchange_ns: 1.into(),
            },
            &PriceFilter::new(
                None,
                None,
                QuoteCurrency::new(1, 5),
                Decimal::TWO,
                Decimal::zero(),
            )
            .unwrap(),
        );
        ms
    }

    fn dec(integer: i64, scale: u8) -> Decimal<i64, 5> {
        Decimal::try_from_scaled(integer, scale).unwrap()
    }

    /// base 0.001, linear 0.02, sqrt 0.01, reference volume 100, cap 5%.
    fn linear_sqrt_model() -> SlippageModel<i64, 5> {
        SlippageModel::linear_sqrt(dec(1, 3), dec(2, 2), dec(1, 2), dec(100, 0), dec(5, 2)).unwrap()
    }

    /// A tick of 0.01.
    fn cent() -> QuoteCurrency<i64, 5> {
        QuoteCurrency::new(1, 2)
    }

    /// The smallest representable tick (0.00001).
    fn ulp() -> QuoteCurrency<i64, 5> {
        QuoteCurrency::new(1, 5)
    }

    #[test]
    fn slippage_models_compute_expected_adverse_prices() {
        let ms = market(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));

        let zero = SlippageModel::<i64, 5>::ZeroImpact;
        let q0 = zero.quote(&ms, Side::Buy, BaseCurrency::new(10, 0), cent());
        assert_eq!(q0.execution_price(), QuoteCurrency::new(101, 0));
        assert!(q0.slippage().is_zero());

        // 1% fixed slippage: Buy @ 101 * 1.01 = 102.01, Sell @ 100 * 0.99 = 99.0
        let fixed = SlippageModel::fixed_rate(dec(1, 2)).unwrap();
        let qb = fixed.quote(&ms, Side::Buy, BaseCurrency::new(10, 0), cent());
        assert_eq!(qb.execution_price(), QuoteCurrency::new(10201, 2));
        assert_eq!(qb.slippage(), QuoteCurrency::new(101, 2));
        let qs = fixed.quote(&ms, Side::Sell, BaseCurrency::new(10, 0), cent());
        assert_eq!(qs.execution_price(), QuoteCurrency::new(99, 0));

        // LinearSqrt: ref_vol = 100, qty = 25 -> participation = 0.25, sqrt = 0.5
        // rate = 0.001 + 0.02 * 0.25 + 0.01 * 0.5 = 0.001 + 0.005 + 0.005 = 0.011
        let ql = linear_sqrt_model().quote(&ms, Side::Sell, BaseCurrency::new(25, 0), cent());
        assert_eq!(ql.slippage_rate(), dec(11, 3));
        assert_eq!(ql.execution_price(), QuoteCurrency::new(989, 1));
        assert_eq!(ql.slippage(), QuoteCurrency::new(11, 1));
    }

    #[test]
    fn linear_sqrt_is_exact_on_perfect_squares() {
        let ms = market(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
        let sqrt_only = SlippageModel::linear_sqrt(
            Decimal::zero(),
            Decimal::zero(),
            dec(1, 2),
            dec(100, 0),
            dec(5, 1),
        )
        .unwrap();
        // participation 1, 4, 9, 0.01, 0.0004 -> sqrt 1, 2, 3, 0.1, 0.02 (times 0.01).
        for (qty, rate) in [
            (BaseCurrency::new(100, 0), dec(1, 2)),
            (BaseCurrency::new(400, 0), dec(2, 2)),
            (BaseCurrency::new(900, 0), dec(3, 2)),
            (BaseCurrency::new(1, 0), dec(1, 3)),
            (BaseCurrency::new(4, 2), dec(2, 4)),
        ] {
            let q = sqrt_only.quote(&ms, Side::Buy, qty, ulp());
            assert_eq!(q.slippage_rate(), rate, "qty {qty}");
        }
        assert_eq!(isqrt_ceil(0), 0);
        assert_eq!(isqrt_ceil(1), 1);
        assert_eq!(isqrt_ceil(144), 12);
        assert_eq!(isqrt_ceil(145), 13);
        assert_eq!(isqrt_ceil(u128::MAX), 1 << 64);
    }

    #[test]
    fn linear_sqrt_rate_rounds_up_against_the_trader() {
        let ms = market(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
        let sqrt_only = SlippageModel::linear_sqrt(
            Decimal::zero(),
            Decimal::zero(),
            dec(1, 0),
            dec(3, 0),
            dec(9, 1),
        )
        .unwrap();
        // sqrt(1/3) = 0.5773502..., rounded up to 5 decimals (round to nearest gives
        // 0.57735, which undercuts the exact rate).
        let q = sqrt_only.quote(&ms, Side::Buy, BaseCurrency::new(1, 0), ulp());
        assert_eq!(q.slippage_rate(), dec(57736, 5));
        let linear_only = SlippageModel::linear_sqrt(
            Decimal::zero(),
            dec(1, 0),
            Decimal::zero(),
            dec(3, 0),
            dec(9, 1),
        )
        .unwrap();
        // 1/3 = 0.333333... -> 0.33334.
        let q = linear_only.quote(&ms, Side::Buy, BaseCurrency::new(1, 0), ulp());
        assert_eq!(q.slippage_rate(), dec(33334, 5));
    }

    #[test]
    fn slippage_amount_rounds_up_and_price_snaps_away_from_the_trader() {
        // Rounding to nearest would favour the trader in both directions here:
        // ask 0.334 * 0.01001 = 0.00334334 -> slippage rounds up to 0.00335,
        // bid 0.333 * 0.01001 = 0.00333333 -> slippage rounds up to 0.00334.
        let ms = market(QuoteCurrency::new(333, 3), QuoteCurrency::new(334, 3));
        let fixed = SlippageModel::fixed_rate(dec(1001, 5)).unwrap();
        let buy = fixed.quote(&ms, Side::Buy, BaseCurrency::new(1, 0), ulp());
        assert_eq!(buy.execution_price(), QuoteCurrency::new(33735, 5));
        assert_eq!(buy.slippage(), QuoteCurrency::new(335, 5));
        let sell = fixed.quote(&ms, Side::Sell, BaseCurrency::new(1, 0), ulp());
        assert_eq!(sell.execution_price(), QuoteCurrency::new(32966, 5));

        // 1% of ask 101 / bid 100 on a grid of 1: buy 102.01 -> 103, sell 99 stays 99.
        let ms = market(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
        let one = QuoteCurrency::new(1, 0);
        let fixed = SlippageModel::fixed_rate(dec(1, 2)).unwrap();
        let buy = fixed.quote(&ms, Side::Buy, BaseCurrency::new(1, 0), one);
        assert_eq!(buy.execution_price(), QuoteCurrency::new(103, 0));
        assert_eq!(buy.slippage(), QuoteCurrency::new(2, 0));
        assert_eq!(
            fixed
                .quote(&ms, Side::Sell, BaseCurrency::new(1, 0), one)
                .execution_price(),
            QuoteCurrency::new(99, 0)
        );
        // 1.5% of bid 100 = 98.5 -> 98 on a grid of 1.
        let fixed = SlippageModel::fixed_rate(dec(15, 3)).unwrap();
        let sell = fixed.quote(&ms, Side::Sell, BaseCurrency::new(1, 0), one);
        assert_eq!(sell.execution_price(), QuoteCurrency::new(98, 0));
        assert_eq!(sell.slippage(), QuoteCurrency::new(2, 0));

        // A sell never executes below one tick.
        let ms = market(QuoteCurrency::new(1, 0), QuoteCurrency::new(2, 0));
        let fixed = SlippageModel::fixed_rate(dec(99, 2)).unwrap();
        let sell = fixed.quote(&ms, Side::Sell, BaseCurrency::new(1, 0), one);
        assert_eq!(sell.execution_price(), one);
    }

    #[test]
    fn linear_sqrt_impact_is_monotonic_in_size_and_capped() {
        let ms = market(QuoteCurrency::new(100, 0), QuoteCurrency::new(101, 0));
        let model = linear_sqrt_model();
        let mut last_rate = Decimal::zero();
        let mut last_buy = QuoteCurrency::zero();
        let mut last_sell = QuoteCurrency::new(1000, 0);
        // Sizes 0.01 ..= 64 (participation up to 0.64), then 64 ..= 6400.
        for i in (1..=800_i64).chain((9..=80).map(|j| j * 100)) {
            let qty = BaseCurrency::new(i * i, 2);
            let buy = model.quote(&ms, Side::Buy, qty, cent());
            let sell = model.quote(&ms, Side::Sell, qty, cent());
            assert!(buy.slippage_rate() >= last_rate, "qty {qty}");
            assert!(buy.slippage_rate() <= dec(5, 2));
            assert!(buy.execution_price() >= last_buy);
            assert!(sell.execution_price() <= last_sell);
            assert!(buy.execution_price() > ms.ask());
            assert!(sell.execution_price() < ms.bid());
            last_rate = buy.slippage_rate();
            last_buy = buy.execution_price();
            last_sell = sell.execution_price();
        }
        // participation 64: 0.001 + 1.28 + 0.08 -> capped at 5%.
        assert_eq!(last_rate, dec(5, 2));
        // Absurd sizes overflow the intermediates and are capped instead of panicking.
        let huge = model.quote(&ms, Side::Buy, BaseCurrency::new(i64::MAX, 5), cent());
        assert_eq!(huge.slippage_rate(), dec(5, 2));
    }

    #[test]
    fn fixed_rate_rejects_rates_outside_the_half_open_unit_interval() {
        // Zero is a valid no-op rate.
        SlippageModel::<i64, 5>::fixed_rate(Decimal::zero()).unwrap();

        assert_eq!(
            SlippageModel::<i64, 5>::fixed_rate(Decimal::one()),
            Err(ConfigError::InvalidSlippageModel)
        );
        assert_eq!(
            SlippageModel::<i64, 5>::fixed_rate(dec(1001, 3)),
            Err(ConfigError::InvalidSlippageModel)
        );
        assert_eq!(
            SlippageModel::<i64, 5>::fixed_rate(dec(-1, 3)),
            Err(ConfigError::InvalidSlippageModel)
        );
    }

    #[test]
    fn linear_sqrt_rejects_each_invalid_parameter_individually() {
        let base = dec(1, 3);
        let linear = dec(2, 2);
        let sqrt = dec(1, 2);
        let volume = dec(100, 0);
        let cap = dec(5, 2);
        SlippageModel::<i64, 5>::linear_sqrt(base, linear, sqrt, volume, cap).unwrap();

        let negative = dec(-1, 3);
        let cases = [
            (negative, linear, sqrt, volume, cap),
            (base, negative, sqrt, volume, cap),
            (base, linear, negative, volume, cap),
            (base, linear, sqrt, Decimal::zero(), cap),
            (base, linear, sqrt, negative, cap),
            (base, linear, sqrt, volume, Decimal::zero()),
            (base, linear, sqrt, volume, Decimal::one()),
            (base, linear, sqrt, volume, dec(1001, 3)),
        ];
        for (idx, (b, l, s, v, c)) in cases.into_iter().enumerate() {
            assert_eq!(
                SlippageModel::<i64, 5>::linear_sqrt(b, l, s, v, c),
                Err(ConfigError::InvalidSlippageModel),
                "invalid parameter combination {idx}"
            );
        }

        // Zero coefficients are allowed; the remaining terms still apply.
        SlippageModel::<i64, 5>::linear_sqrt(
            Decimal::zero(),
            Decimal::zero(),
            Decimal::zero(),
            volume,
            cap,
        )
        .unwrap();
    }
}
