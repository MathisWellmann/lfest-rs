//! Pluggable market-impact and slippage models for aggressive executions
//! (adapted from `OrderBook-rs` `market_impact.rs`).

use const_decimal::Decimal;
use getset::CopyGetters;
use num_traits::{
    One,
    Zero,
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

    /// Adverse slippage expressed as a fractional rate (`slippage / reference_price`).
    #[getset(get_copy = "pub")]
    slippage_rate: Decimal<I, D>,
}

/// Pluggable execution slippage / market-impact model for aggressive orders.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
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
    /// of `(side, quantity)` given `market_state`.
    #[inline]
    #[must_use]
    pub fn quote<BaseOrQuote: Currency<I, D>>(
        &self,
        market_state: &MarketState<I, D>,
        side: Side,
        quantity: BaseOrQuote,
    ) -> MarketImpactQuote<I, D> {
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
                let qty_f = quantity.as_ref().to_f64().abs();
                let vol_f = reference_volume.to_f64();
                let participation = (qty_f / vol_f).max(0.0);
                let rate_f = base_rate.to_f64()
                    + linear_coeff.to_f64() * participation
                    + sqrt_coeff.to_f64() * participation.sqrt();
                let capped = rate_f.clamp(0.0, max_impact_rate.to_f64());
                crate::utils::decimal_from_f64(capped).unwrap_or(max_impact_rate)
            }
        };

        if impact_rate.is_zero() {
            return MarketImpactQuote {
                reference_price,
                execution_price: reference_price,
                slippage: QuoteCurrency::zero(),
                slippage_rate: Decimal::zero(),
            };
        }

        let slippage = reference_price * impact_rate;
        let execution_price = match side {
            Side::Buy => reference_price + slippage,
            Side::Sell => {
                if slippage >= reference_price {
                    QuoteCurrency::zero()
                } else {
                    reference_price - slippage
                }
            }
        };

        MarketImpactQuote {
            reference_price,
            execution_price,
            slippage,
            slippage_rate: impact_rate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::{
        BaseCurrency,
        Bba,
        PriceFilter,
    };

    #[test]
    fn slippage_models_compute_expected_adverse_prices() {
        let mut ms = MarketState::<i64, 5>::default();
        ms.update_state::<_, BaseCurrency<i64, 5>>(
            &Bba {
                bid: QuoteCurrency::new(100, 0),
                ask: QuoteCurrency::new(101, 0),
                timestamp_exchange_ns: 1.into(),
            },
            &PriceFilter::default(),
        );

        let zero = SlippageModel::<i64, 5>::ZeroImpact;
        let q0 = zero.quote(&ms, Side::Buy, BaseCurrency::new(10, 0));
        assert_eq!(q0.execution_price(), QuoteCurrency::new(101, 0));
        assert!(q0.slippage().is_zero());

        // 1% fixed slippage: Buy @ 101 * 1.01 = 102.01, Sell @ 100 * 0.99 = 99.0
        let fixed = SlippageModel::fixed_rate(Decimal::try_from_scaled(1, 2).unwrap()).unwrap();
        let qb = fixed.quote(&ms, Side::Buy, BaseCurrency::new(10, 0));
        assert_eq!(qb.execution_price(), QuoteCurrency::new(10201, 2));
        let qs = fixed.quote(&ms, Side::Sell, BaseCurrency::new(10, 0));
        assert_eq!(qs.execution_price(), QuoteCurrency::new(99, 0));

        // LinearSqrt: ref_vol = 100, qty = 25 -> participation = 0.25, sqrt = 0.5
        // rate = 0.001 + 0.02 * 0.25 + 0.01 * 0.5 = 0.001 + 0.005 + 0.005 = 0.011
        let ls = SlippageModel::linear_sqrt(
            Decimal::try_from_scaled(1, 3).unwrap(),
            Decimal::try_from_scaled(2, 2).unwrap(),
            Decimal::try_from_scaled(1, 2).unwrap(),
            Decimal::try_from_scaled(100, 0).unwrap(),
            Decimal::try_from_scaled(5, 2).unwrap(),
        )
        .unwrap();
        let ql = ls.quote(&ms, Side::Sell, BaseCurrency::new(25, 0));
        assert_eq!(ql.slippage_rate(), Decimal::try_from_scaled(11, 3).unwrap());
        assert_eq!(ql.execution_price(), QuoteCurrency::new(989, 1));
    }
}
